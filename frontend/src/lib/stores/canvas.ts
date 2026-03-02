/** Store for the spatial canvas state. */

import { writable, get } from 'svelte/store';
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
}

const ZOOM_MIN = 0.1;
const ZOOM_MAX = 5.0;
const CARD_W = 200;
const CARD_H = 80;
const LANE_PADDING = 40;

function createCanvasStore() {
	const initial: CanvasState = {
		documents: [],
		threads: [],
		relationships: [],
		milestones: [],
		camera: { panX: 0, panY: 0, zoom: 1 },
		hoveredCardId: null,
		selectedCardId: null,
		draggingCardId: null,
		loaded: false
	};

	const { subscribe, update, set } = writable<CanvasState>(initial);

	/** Position save debounce timer */
	let positionTimer: ReturnType<typeof setTimeout> | null = null;

	return {
		subscribe,

		/** Load canvas data from backend. */
		async load() {
			try {
				const data = await canvasLoad();
				// Auto-layout documents that have no saved position
				const docs = autoLayout(data.documents, data.threads);
				update((s) => ({
					...s,
					documents: docs,
					threads: data.threads,
					relationships: data.relationships,
					milestones: data.milestones,
					loaded: true
				}));
				// Compute home position
				this.home();
			} catch (e) {
				console.error('Failed to load canvas:', e);
			}
		},

		/** Refresh canvas data (re-fetch). */
		async refresh() {
			const state = get({ subscribe });
			if (!state.loaded) return;
			try {
				const data = await canvasLoad();
				const docs = autoLayout(data.documents, data.threads);
				update((s) => ({
					...s,
					documents: docs,
					threads: data.threads,
					relationships: data.relationships,
					milestones: data.milestones
				}));
			} catch (e) {
				console.error('Failed to refresh canvas:', e);
			}
		},

		/** Pan the camera by a delta. */
		panBy(dx: number, dy: number) {
			update((s) => ({
				...s,
				camera: { ...s.camera, panX: s.camera.panX + dx, panY: s.camera.panY + dy }
			}));
		},

		/** Zoom at a specific screen point. */
		zoomAt(screenX: number, screenY: number, delta: number) {
			update((s) => {
				const oldZoom = s.camera.zoom;
				const factor = delta > 0 ? 0.9 : 1.1;
				const newZoom = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, oldZoom * factor));
				// Adjust pan to keep point under cursor stable
				const ratio = newZoom / oldZoom;
				const panX = screenX - ratio * (screenX - s.camera.panX);
				const panY = screenY - ratio * (screenY - s.camera.panY);
				return { ...s, camera: { panX, panY, zoom: newZoom } };
			});
		},

		/** Jump camera to show all documents centered. */
		home() {
			update((s) => {
				if (s.documents.length === 0) {
					return { ...s, camera: { panX: 0, panY: 0, zoom: 1 } };
				}
				let minX = Infinity,
					minY = Infinity,
					maxX = -Infinity,
					maxY = -Infinity;
				for (const d of s.documents) {
					minX = Math.min(minX, d.spatial_x);
					minY = Math.min(minY, d.spatial_y);
					maxX = Math.max(maxX, d.spatial_x + CARD_W);
					maxY = Math.max(maxY, d.spatial_y + CARD_H);
				}
				const cx = (minX + maxX) / 2;
				const cy = (minY + maxY) / 2;
				// Center on screen (assume viewport fills window minus taskbar)
				const vw = typeof window !== 'undefined' ? window.innerWidth : 1200;
				const vh = typeof window !== 'undefined' ? window.innerHeight - 44 : 700;
				const zoom = Math.min(1, vw / (maxX - minX + 200), vh / (maxY - minY + 200));
				return {
					...s,
					camera: {
						panX: vw / 2 - cx * zoom,
						panY: vh / 2 - cy * zoom,
						zoom
					}
				};
			});
		},

		/** Move a card to new world coordinates. */
		moveCard(id: string, x: number, y: number) {
			update((s) => ({
				...s,
				documents: s.documents.map((d) =>
					d.id === id ? { ...d, spatial_x: x, spatial_y: y } : d
				)
			}));
			// Debounce position save to backend
			if (positionTimer) clearTimeout(positionTimer);
			positionTimer = setTimeout(() => {
				updateDocumentPosition(id, x, y).catch((e) =>
					console.error('Failed to save position:', e)
				);
			}, 500);
		},

		/** Select a card. */
		selectCard(id: string | null) {
			update((s) => ({ ...s, selectedCardId: id }));
		},

		/** Hover a card. */
		hoverCard(id: string | null) {
			update((s) => ({ ...s, hoveredCardId: id }));
		},

		/** Set dragging state. */
		setDragging(id: string | null) {
			update((s) => ({ ...s, draggingCardId: id }));
		},

		/** Navigate to and select a document by ID. */
		navigateToDoc(id: string) {
			update((s) => {
				const doc = s.documents.find((d) => d.id === id);
				if (!doc) return s;
				const vw = typeof window !== 'undefined' ? window.innerWidth : 1200;
				const vh = typeof window !== 'undefined' ? window.innerHeight - 44 : 700;
				return {
					...s,
					camera: {
						...s.camera,
						panX: vw / 2 - doc.spatial_x * s.camera.zoom,
						panY: vh / 2 - doc.spatial_y * s.camera.zoom
					},
					selectedCardId: id
				};
			});
		}
	};
}

/** Auto-layout documents that have no saved position (spatial_x == 0 && spatial_y == 0). */
function autoLayout(docs: CanvasDocDto[], threads: ThreadDto[]): CanvasDocDto[] {
	// Build thread order
	const threadOrder = new Map<string, number>();
	threads.forEach((t, i) => threadOrder.set(t.id, i));

	// Group docs by thread
	const byThread = new Map<string, CanvasDocDto[]>();
	for (const d of docs) {
		const list = byThread.get(d.thread_id) || [];
		list.push(d);
		byThread.set(d.thread_id, list);
	}

	const result: CanvasDocDto[] = [];
	let laneY = 0;

	// Layout each thread lane
	const sortedThreadIds = [...byThread.keys()].sort(
		(a, b) => (threadOrder.get(a) ?? 999) - (threadOrder.get(b) ?? 999)
	);

	for (const tid of sortedThreadIds) {
		const threadDocs = byThread.get(tid) || [];
		let col = 0;
		for (const d of threadDocs) {
			if (d.spatial_x === 0 && d.spatial_y === 0) {
				// Auto-position: place in lane grid
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

export const canvas = createCanvasStore();
