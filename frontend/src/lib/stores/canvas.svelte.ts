/** Rune-based reactive state for the spatial canvas. */

import {
	canvasLoad,
	updateDocumentPosition,
	type CanvasDocDto,
	type ThreadDto,
	type RelationshipDto,
	type MilestoneDto
} from '$lib/api/commands';

export interface Camera {
	panX: number;
	panY: number;
	zoom: number;
}

export interface CanvasState {
	documents: CanvasDocDto[];
	threads: ThreadDto[];
	relationships: RelationshipDto[];
	milestones: MilestoneDto[];
	camera: Camera;
	hoveredCardId: string | null;
	selectedCardId: string | null;
	draggingCardId: string | null;
	loaded: boolean;
	loadError: string | null;
}

const ZOOM_MIN = 0.1;
const ZOOM_MAX = 5.0;
const CARD_W = 200;
const CARD_H = 80;
const LANE_PADDING = 40;

/** Reactive canvas state — $state() creates a deep Proxy for fine-grained tracking. */
export const canvas: CanvasState = $state({
	documents: [],
	threads: [],
	relationships: [],
	milestones: [],
	camera: { panX: 0, panY: 0, zoom: 1 },
	hoveredCardId: null,
	selectedCardId: null,
	draggingCardId: null,
	loaded: false,
	loadError: null
});

/** Position save debounce timer */
let positionTimer: ReturnType<typeof setTimeout> | null = null;

/** Load canvas data from backend. */
export async function load() {
	try {
		const data = await canvasLoad();
		const docs = autoLayout(data.documents, data.threads);
		canvas.documents = docs;
		canvas.threads = data.threads;
		canvas.relationships = data.relationships;
		canvas.milestones = data.milestones;
		canvas.loaded = true;
		canvas.loadError = null;
		home();
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
		const docs = autoLayout(data.documents, data.threads);
		canvas.documents = docs;
		canvas.threads = data.threads;
		canvas.relationships = data.relationships;
		canvas.milestones = data.milestones;
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

/** Jump camera to show all documents centered. */
export function home() {
	if (canvas.documents.length === 0) {
		canvas.camera.panX = 0;
		canvas.camera.panY = 0;
		canvas.camera.zoom = 1;
		return;
	}
	let minX = Infinity,
		minY = Infinity,
		maxX = -Infinity,
		maxY = -Infinity;
	for (const d of canvas.documents) {
		minX = Math.min(minX, d.spatial_x);
		minY = Math.min(minY, d.spatial_y);
		maxX = Math.max(maxX, d.spatial_x + CARD_W);
		maxY = Math.max(maxY, d.spatial_y + CARD_H);
	}
	const cx = (minX + maxX) / 2;
	const cy = (minY + maxY) / 2;
	const vw = typeof window !== 'undefined' ? window.innerWidth : 1200;
	const vh = typeof window !== 'undefined' ? window.innerHeight - 44 : 700;
	const zoom = Math.min(1, vw / (maxX - minX + 200), vh / (maxY - minY + 200));
	canvas.camera.panX = vw / 2 - cx * zoom;
	canvas.camera.panY = vh / 2 - cy * zoom;
	canvas.camera.zoom = zoom;
}

/** Move a card to new world coordinates. */
export function moveCard(id: string, x: number, y: number) {
	const doc = canvas.documents.find((d) => d.id === id);
	if (doc) {
		doc.spatial_x = x;
		doc.spatial_y = y;
	}
	if (positionTimer) clearTimeout(positionTimer);
	positionTimer = setTimeout(() => {
		updateDocumentPosition(id, x, y).catch((e) =>
			console.error('Failed to save position:', e)
		);
	}, 500);
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

/** Auto-layout documents that have no saved position (spatial_x == 0 && spatial_y == 0). */
function autoLayout(docs: CanvasDocDto[], threads: ThreadDto[]): CanvasDocDto[] {
	const threadOrder = new Map<string, number>();
	threads.forEach((t, i) => threadOrder.set(t.id, i));

	const byThread = new Map<string, CanvasDocDto[]>();
	for (const d of docs) {
		const list = byThread.get(d.thread_id) || [];
		list.push(d);
		byThread.set(d.thread_id, list);
	}

	const result: CanvasDocDto[] = [];
	let laneY = 0;

	const sortedThreadIds = [...byThread.keys()].sort(
		(a, b) => (threadOrder.get(a) ?? 999) - (threadOrder.get(b) ?? 999)
	);

	for (const tid of sortedThreadIds) {
		const threadDocs = byThread.get(tid) || [];
		let col = 0;
		for (const d of threadDocs) {
			if (d.spatial_x === 0 && d.spatial_y === 0) {
				result.push({
					...d,
					spatial_x: 200 + col * (CARD_W + 20),
					spatial_y: laneY + LANE_PADDING
				});
			} else {
				result.push(d);
			}
			col++;
		}
		laneY += CARD_H + LANE_PADDING * 2;
	}

	return result;
}
