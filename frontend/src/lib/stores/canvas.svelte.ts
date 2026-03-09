/** Rune-based reactive state for the timeline canvas. */

import {
	canvasLoad,
	canvasLoadMessages,
	updateDocumentPosition,
	moveDocumentToThread,
	type CanvasDocDto,
	type ThreadDto,
	type RelationshipDto,
	type MilestoneDto,
	type CanvasMessageDto
} from '$lib/api/commands';

export interface Camera {
	panX: number;
	panY: number;
	zoom: number;
}

export interface PositionedMessage extends CanvasMessageDto {
	x: number;
	y: number;
}

export interface TimelineScale {
	minDate: number; // ms — earliest modified_at minus padding
	maxDate: number; // ms — max(now, latest modified_at) plus padding
	pxPerMs: number; // pixels per millisecond
	originX: number; // pixel offset of minDate (= LABEL_MARGIN)
	nowX: number; // pixel X of "Now" line
}

export interface CanvasState {
	documents: CanvasDocDto[];
	threads: ThreadDto[];
	relationships: RelationshipDto[];
	milestones: MilestoneDto[];
	messages: PositionedMessage[];
	camera: Camera;
	hoveredCardId: string | null;
	selectedCardId: string | null;
	draggingCardId: string | null;
	loaded: boolean;
	loadError: string | null;
	timelineScale: TimelineScale | null;
}

const ZOOM_MIN = 0.02;
const ZOOM_MAX = 5.0;
export const CARD_W = 200;
export const CARD_H = 80;
export const LANE_HEIGHT = 120;
export const MSG_RADIUS = 30;

/** Left margin reserved for thread labels. */
const LABEL_MARGIN = 200;
/** Pixels per day at zoom = 1. */
const PX_PER_DAY = 120;
/** One day in milliseconds. */
const MS_PER_DAY = 86_400_000;

/** Reactive canvas state — $state() creates a deep Proxy for fine-grained tracking. */
export const canvas: CanvasState = $state({
	documents: [],
	threads: [],
	relationships: [],
	milestones: [],
	messages: [],
	camera: { panX: 0, panY: 0, zoom: 1 },
	hoveredCardId: null,
	selectedCardId: null,
	draggingCardId: null,
	loaded: false,
	loadError: null,
	timelineScale: null
});

/** Interval handle for periodic "Now" line updates. */
let nowTimer: ReturnType<typeof setInterval> | null = null;

/** Load canvas data from backend. */
export async function load() {
	try {
		const data = await canvasLoad();
		const docs = timelineLayout(data.documents, data.threads);
		canvas.documents = docs;
		canvas.threads = data.threads;
		canvas.relationships = data.relationships;
		canvas.milestones = data.milestones;
		canvas.messages = []; // loaded separately via viewport-scoped requestMessagesForViewport()
		canvas.loaded = true;
		canvas.loadError = null;
		home(); // triggers $effect → requestMessagesForViewport()
		startNowTimer();
	} catch (e) {
		console.error('Failed to load canvas:', e);
		canvas.loadError = String(e);
		canvas.loaded = true;
	}
}

/** Refresh canvas data (re-fetch). */
export async function refresh() {
	if (!canvas.loaded) return;
	try {
		const data = await canvasLoad();
		const docs = timelineLayout(data.documents, data.threads);
		canvas.documents = docs;
		canvas.threads = data.threads;
		canvas.relationships = data.relationships;
		canvas.milestones = data.milestones;
		// Messages will be refreshed by the viewport $effect
		requestMessagesForViewport();
	} catch (e) {
		console.error('Failed to refresh canvas:', e);
	}
}

/** Pan the camera by a delta. */
export function panBy(dx: number, dy: number) {
	canvas.camera.panX += dx;
	canvas.camera.panY += dy;
}

/** Zoom at a specific screen point. */
export function zoomAt(screenX: number, screenY: number, delta: number) {
	const oldZoom = canvas.camera.zoom;
	const factor = delta > 0 ? 0.9 : 1.1;
	const newZoom = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, oldZoom * factor));
	const ratio = newZoom / oldZoom;
	canvas.camera.panX = screenX - ratio * (screenX - canvas.camera.panX);
	canvas.camera.panY = screenY - ratio * (screenY - canvas.camera.panY);
	canvas.camera.zoom = newZoom;
}

/** Jump camera to center on "Now" with a readable zoom level. */
export function home() {
	const scale = canvas.timelineScale;
	const vw = typeof window !== 'undefined' ? window.innerWidth : 1200;
	const vh = typeof window !== 'undefined' ? window.innerHeight - 44 : 700;

	if (!scale || canvas.documents.length === 0) {
		canvas.camera.panX = vw / 2;
		canvas.camera.panY = 0;
		canvas.camera.zoom = 1;
		return;
	}

	const totalHeight = canvas.threads.length * LANE_HEIGHT;
	// Zoom: fit all lanes vertically, clamp to 0.6 minimum so titles are readable
	const zoom = Math.min(1, Math.max(0.6, vh / (totalHeight + 100)));
	// Center horizontally on "Now" line
	canvas.camera.panX = vw / 2 - scale.nowX * zoom;
	// Center vertically on all lanes
	canvas.camera.panY = (vh - totalHeight * zoom) / 2;
	canvas.camera.zoom = zoom;
}

/** Move a card vertically (X stays locked to timeline). */
export function moveCard(id: string, _x: number, y: number) {
	const doc = canvas.documents.find((d) => d.id === id);
	if (doc) {
		doc.spatial_y = y;
	}
}

/** Select a card. */
export function selectCard(id: string | null) {
	canvas.selectedCardId = id;
}

/** Hover a card. */
export function hoverCard(id: string | null) {
	canvas.hoveredCardId = id;
}

/** Set dragging state. */
export function setDragging(id: string | null) {
	canvas.draggingCardId = id;
}

/** Snap a card to the closest lane center after a drag ends. Updates thread if changed. */
export function snapToLane(id: string) {
	const doc = canvas.documents.find((d) => d.id === id);
	if (!doc || canvas.threads.length === 0) return;

	const cardCenterY = doc.spatial_y + CARD_H / 2;
	let closestIdx = 0;
	let closestDist = Infinity;
	for (let i = 0; i < canvas.threads.length; i++) {
		const laneCenterY = i * LANE_HEIGHT + LANE_HEIGHT / 2;
		const dist = Math.abs(cardCenterY - laneCenterY);
		if (dist < closestDist) {
			closestDist = dist;
			closestIdx = i;
		}
	}

	const snappedY = closestIdx * LANE_HEIGHT + (LANE_HEIGHT - CARD_H) / 2;
	doc.spatial_y = snappedY;

	const newThread = canvas.threads[closestIdx];
	if (newThread && doc.thread_id !== newThread.id) {
		doc.thread_id = newThread.id;
		moveDocumentToThread(doc.id, newThread.id).catch((e) =>
			console.error('Failed to move document to thread:', e)
		);
	}

	updateDocumentPosition(id, doc.spatial_x, snappedY).catch((e) =>
		console.error('Failed to save position:', e)
	);
}

/** Navigate to and select a document by ID. */
export function navigateToDoc(id: string) {
	const doc = canvas.documents.find((d) => d.id === id);
	if (!doc) return;
	const vw = typeof window !== 'undefined' ? window.innerWidth : 1200;
	const vh = typeof window !== 'undefined' ? window.innerHeight - 44 : 700;
	canvas.camera.panX = vw / 2 - doc.spatial_x * canvas.camera.zoom;
	canvas.camera.panY = vh / 2 - doc.spatial_y * canvas.camera.zoom;
	canvas.selectedCardId = id;
}

// ---------------------------------------------------------------------------
// Viewport culling
// ---------------------------------------------------------------------------

/** World-space padding to avoid card pop-in at viewport edges. */
const VIEWPORT_PAD = 300;

/** Compute viewport bounds in world-space coordinates. */
export function computeViewport() {
	const vw = typeof window !== 'undefined' ? window.innerWidth : 1200;
	const vh = typeof window !== 'undefined' ? window.innerHeight - 44 : 800;
	const { panX, panY, zoom } = canvas.camera;
	return {
		left: -panX / zoom - VIEWPORT_PAD,
		right: (-panX + vw) / zoom + VIEWPORT_PAD,
		top: -panY / zoom - VIEWPORT_PAD,
		bottom: (-panY + vh) / zoom + VIEWPORT_PAD
	};
}

/** Return only the documents whose bounding box intersects the current viewport.
 *  At zoom < 0.15 (heatmap mode), returns empty — no DOM cards are rendered. */
export function getVisibleDocuments(): CanvasDocDto[] {
	if (canvas.camera.zoom < 0.15) return [];
	const vp = computeViewport();
	return canvas.documents.filter(
		(d) =>
			d.spatial_x + CARD_W >= vp.left &&
			d.spatial_x <= vp.right &&
			d.spatial_y + CARD_H >= vp.top &&
			d.spatial_y <= vp.bottom
	);
}

// ---------------------------------------------------------------------------
// Timeline layout
// ---------------------------------------------------------------------------

/** Compute the timeline scale from a set of document dates. */
function computeScale(docs: CanvasDocDto[]): TimelineScale {
	const now = Date.now();

	if (docs.length === 0) {
		// Empty canvas — center on "now" with a 7-day window
		const minDate = now - 3 * MS_PER_DAY;
		const maxDate = now + 3 * MS_PER_DAY;
		const pxPerMs = PX_PER_DAY / MS_PER_DAY;
		return {
			minDate,
			maxDate,
			pxPerMs,
			originX: LABEL_MARGIN,
			nowX: LABEL_MARGIN + (now - minDate) * pxPerMs
		};
	}

	let earliest = now;
	let latest = now;
	for (const d of docs) {
		const t = new Date(d.modified_at).getTime();
		if (t < earliest) earliest = t;
		if (t > latest) latest = t;
	}

	// Add 1-day padding on each side
	const minDate = earliest - MS_PER_DAY;
	const maxDate = Math.max(latest, now) + MS_PER_DAY;
	const pxPerMs = PX_PER_DAY / MS_PER_DAY;

	return {
		minDate,
		maxDate,
		pxPerMs,
		originX: LABEL_MARGIN,
		nowX: LABEL_MARGIN + (now - minDate) * pxPerMs
	};
}

/** Cascade offset for stacked cards. */
const STACK_OFFSET_X = 20;
const STACK_OFFSET_Y = 10;

/** Position documents on the timeline with cascade stacking for same-date cards. */
function timelineLayout(docs: CanvasDocDto[], threads: ThreadDto[]): CanvasDocDto[] {
	const scale = computeScale(docs);
	canvas.timelineScale = scale;

	const threadOrder = new Map<string, number>();
	threads.forEach((t, i) => threadOrder.set(t.id, i));

	// Group by lane, sort by date within each lane
	const byLane = new Map<number, { doc: CanvasDocDto; baseX: number }[]>();
	for (const d of docs) {
		const laneIdx = threadOrder.get(d.thread_id) ?? 0;
		const t = new Date(d.modified_at).getTime();
		const baseX = scale.originX + (t - scale.minDate) * scale.pxPerMs;
		const list = byLane.get(laneIdx) || [];
		list.push({ doc: d, baseX });
		byLane.set(laneIdx, list);
	}

	const result: CanvasDocDto[] = [];
	for (const [laneIdx, entries] of byLane) {
		entries.sort((a, b) => a.baseX - b.baseX);
		const baseY = laneIdx * LANE_HEIGHT + (LANE_HEIGHT - CARD_H) / 2;
		const placed: { x: number }[] = [];

		for (const { doc, baseX } of entries) {
			// Count overlapping earlier cards
			let stackIdx = 0;
			for (const p of placed) {
				if (Math.abs(baseX - p.x) < CARD_W) stackIdx++;
			}
			const x = baseX + stackIdx * STACK_OFFSET_X;
			const y = baseY + stackIdx * STACK_OFFSET_Y;
			placed.push({ x: baseX });
			result.push({ ...doc, spatial_x: x, spatial_y: y });
		}
	}

	return result;
}

/** Position messages on the timeline by sent_at. */
function layoutMessages(
	msgs: CanvasMessageDto[],
	threads: ThreadDto[]
): PositionedMessage[] {
	const scale = canvas.timelineScale;
	if (!scale || msgs.length === 0) return [];

	const threadOrder = new Map<string, number>();
	threads.forEach((t, i) => threadOrder.set(t.id, i));

	return msgs.map((m) => {
		const t = new Date(m.sent_at).getTime();
		const x = scale.originX + (t - scale.minDate) * scale.pxPerMs;
		const laneIdx = threadOrder.get(m.thread_id) ?? 0;
		const y = laneIdx * LANE_HEIGHT + LANE_HEIGHT / 2;
		return { ...m, x, y };
	});
}

/** Start periodic "Now" line refresh (every 10 minutes). */
function startNowTimer() {
	if (nowTimer) clearInterval(nowTimer);
	nowTimer = setInterval(() => {
		if (canvas.timelineScale) {
			const { minDate, pxPerMs, originX } = canvas.timelineScale;
			canvas.timelineScale.nowX = originX + (Date.now() - minDate) * pxPerMs;
		}
	}, 600_000);
}

/** Stop the "Now" line refresh timer. Call on unmount to prevent leaks. */
export function stopNowTimer() {
	if (nowTimer) {
		clearInterval(nowTimer);
		nowTimer = null;
	}
}

// ---------------------------------------------------------------------------
// Viewport-scoped message loading
// ---------------------------------------------------------------------------

let messageDebounce: ReturnType<typeof setTimeout> | null = null;

/** Request messages for the current viewport time range (debounced 200ms). */
export function requestMessagesForViewport() {
	if (messageDebounce) clearTimeout(messageDebounce);
	messageDebounce = setTimeout(async () => {
		const vp = computeViewport();
		const scale = canvas.timelineScale;
		if (!scale) return;
		// Convert world X → time
		const tMinMs = scale.minDate + (vp.left - scale.originX) / scale.pxPerMs;
		const tMaxMs = scale.minDate + (vp.right - scale.originX) / scale.pxPerMs;
		const tMin = new Date(tMinMs).toISOString();
		const tMax = new Date(tMaxMs).toISOString();
		try {
			const msgs = await canvasLoadMessages(tMin, tMax);
			canvas.messages = layoutMessages(msgs, canvas.threads);
		} catch (e) {
			console.error('Failed to load viewport messages:', e);
		}
	}, 200);
}
