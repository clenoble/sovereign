import { beforeEach, describe, expect, it } from 'vitest';
import type { CanvasDocDto } from '$lib/api/commands';
import {
	canvas,
	computeViewport,
	getVisibleDocuments,
	panBy,
	zoomAt,
	CARD_W,
	CARD_H
} from './canvas.svelte';

const ZOOM_MIN = 0.02;
const ZOOM_MAX = 20.0; // keep in sync with canvas.svelte.ts (raised 5 -> 20 for minute-level zoom)
const VIEWPORT_PAD = 300;
const HEATMAP_THRESHOLD = 0.15;

function makeDoc(overrides: Partial<CanvasDocDto> = {}): CanvasDocDto {
	return {
		id: 'doc:1',
		title: 'Test',
		thread_id: 't:1',
		is_owned: true,
		spatial_x: 0,
		spatial_y: 0,
		created_at: '2026-01-01T00:00:00Z',
		modified_at: '2026-01-01T00:00:00Z',
		reliability_classification: null,
		reliability_score: null,
		source_url: null,
		...overrides
	};
}

beforeEach(() => {
	canvas.documents = [];
	canvas.threads = [];
	canvas.relationships = [];
	canvas.milestones = [];
	canvas.messages = [];
	canvas.camera = { panX: 0, panY: 0, zoom: 1 };
	canvas.hoveredCardId = null;
	canvas.selectedCardId = null;
	canvas.draggingCardId = null;
	canvas.loaded = false;
	canvas.loadError = null;
	canvas.timelineScale = null;
});

describe('panBy', () => {
	it('increments panX and panY by the given deltas', () => {
		canvas.camera = { panX: 10, panY: 20, zoom: 1 };
		panBy(5, -3);
		expect(canvas.camera.panX).toBe(15);
		expect(canvas.camera.panY).toBe(17);
	});

	it('does not change zoom', () => {
		canvas.camera = { panX: 0, panY: 0, zoom: 0.5 };
		panBy(100, 100);
		expect(canvas.camera.zoom).toBe(0.5);
	});
});

describe('zoomAt', () => {
	it('zooms in when delta < 0', () => {
		canvas.camera = { panX: 0, panY: 0, zoom: 1 };
		zoomAt(0, 0, -1);
		expect(canvas.camera.zoom).toBeGreaterThan(1);
	});

	it('zooms out when delta > 0', () => {
		canvas.camera = { panX: 0, panY: 0, zoom: 1 };
		zoomAt(0, 0, 1);
		expect(canvas.camera.zoom).toBeLessThan(1);
	});

	it('clamps to ZOOM_MAX when zooming in past the cap', () => {
		canvas.camera = { panX: 0, panY: 0, zoom: 1 };
		for (let i = 0; i < 100; i++) zoomAt(0, 0, -1);
		expect(canvas.camera.zoom).toBe(ZOOM_MAX);
	});

	it('clamps to ZOOM_MIN when zooming out past the floor', () => {
		canvas.camera = { panX: 0, panY: 0, zoom: 1 };
		for (let i = 0; i < 200; i++) zoomAt(0, 0, 1);
		expect(canvas.camera.zoom).toBeCloseTo(ZOOM_MIN, 5);
	});

	it('preserves the world point under the cursor (zoom-on-cursor invariant)', () => {
		canvas.camera = { panX: 100, panY: 50, zoom: 1 };
		const screenX = 400;
		const screenY = 300;
		const worldXBefore = (screenX - canvas.camera.panX) / canvas.camera.zoom;
		const worldYBefore = (screenY - canvas.camera.panY) / canvas.camera.zoom;

		zoomAt(screenX, screenY, -1);

		const worldXAfter = (screenX - canvas.camera.panX) / canvas.camera.zoom;
		const worldYAfter = (screenY - canvas.camera.panY) / canvas.camera.zoom;

		expect(worldXAfter).toBeCloseTo(worldXBefore, 5);
		expect(worldYAfter).toBeCloseTo(worldYBefore, 5);
	});

	it('preserves the cursor invariant when zooming out', () => {
		canvas.camera = { panX: -200, panY: 100, zoom: 0.8 };
		const screenX = 600;
		const screenY = 450;
		const worldXBefore = (screenX - canvas.camera.panX) / canvas.camera.zoom;
		const worldYBefore = (screenY - canvas.camera.panY) / canvas.camera.zoom;

		zoomAt(screenX, screenY, 1);

		const worldXAfter = (screenX - canvas.camera.panX) / canvas.camera.zoom;
		const worldYAfter = (screenY - canvas.camera.panY) / canvas.camera.zoom;

		expect(worldXAfter).toBeCloseTo(worldXBefore, 5);
		expect(worldYAfter).toBeCloseTo(worldYBefore, 5);
	});
});

describe('computeViewport', () => {
	it('extends VIEWPORT_PAD beyond the visible window in all directions', () => {
		canvas.camera = { panX: 0, panY: 0, zoom: 1 };
		const vp = computeViewport();
		expect(vp.left).toBe(-VIEWPORT_PAD);
		expect(vp.top).toBe(-VIEWPORT_PAD);
	});

	it('returns a strictly smaller world span when zoomed in', () => {
		canvas.camera = { panX: 0, panY: 0, zoom: 1 };
		const wide = computeViewport();
		const wideSpan = wide.right - wide.left;

		canvas.camera = { panX: 0, panY: 0, zoom: 2 };
		const narrow = computeViewport();
		const narrowSpan = narrow.right - narrow.left;

		expect(narrowSpan).toBeLessThan(wideSpan);
	});

	it('shifts viewport when camera pans (right pan moves world view left)', () => {
		canvas.camera = { panX: 0, panY: 0, zoom: 1 };
		const before = computeViewport();

		// Panning the camera right by 200 (positive panX) shifts what's visible
		// to lower world-X values: the new viewport's left edge moves left by 200.
		canvas.camera = { panX: 200, panY: 0, zoom: 1 };
		const after = computeViewport();

		expect(after.left).toBe(before.left - 200);
	});
});

describe('getVisibleDocuments', () => {
	it('returns [] in heatmap mode (zoom below threshold) regardless of doc positions', () => {
		canvas.documents = [makeDoc({ id: 'doc:visible', spatial_x: 0, spatial_y: 0 })];
		canvas.camera = { panX: 0, panY: 0, zoom: HEATMAP_THRESHOLD - 0.01 };
		expect(getVisibleDocuments()).toEqual([]);
	});

	it('includes docs whose bounds intersect the viewport', () => {
		canvas.documents = [
			makeDoc({ id: 'doc:in-view', spatial_x: 100, spatial_y: 100 }),
			makeDoc({ id: 'doc:far-away', spatial_x: 100_000, spatial_y: 100_000 })
		];
		canvas.camera = { panX: 0, panY: 0, zoom: 1 };

		const visibleIds = getVisibleDocuments().map((d) => d.id);
		expect(visibleIds).toContain('doc:in-view');
		expect(visibleIds).not.toContain('doc:far-away');
	});

	it('includes a doc straddling the viewport edge (within VIEWPORT_PAD)', () => {
		// At zoom=1, panX=0: viewport.left = -VIEWPORT_PAD = -300.
		// A card at spatial_x = -250 has its right edge at -250 + CARD_W = -250+200 = -50.
		// That right edge is >= viewport.left (-300), so the card should be visible.
		canvas.documents = [
			makeDoc({ id: 'doc:edge', spatial_x: -250, spatial_y: 0 })
		];
		canvas.camera = { panX: 0, panY: 0, zoom: 1 };
		const visible = getVisibleDocuments();
		expect(visible.map((d) => d.id)).toContain('doc:edge');
	});

	it('excludes a doc fully past the right edge (beyond VIEWPORT_PAD)', () => {
		canvas.documents = [
			makeDoc({ id: 'doc:beyond', spatial_x: 999_999, spatial_y: 0 })
		];
		canvas.camera = { panX: 0, panY: 0, zoom: 1 };
		expect(getVisibleDocuments().map((d) => d.id)).not.toContain('doc:beyond');
	});
});

describe('canvas constants (regression guard)', () => {
	it('CARD_W and CARD_H are positive', () => {
		expect(CARD_W).toBeGreaterThan(0);
		expect(CARD_H).toBeGreaterThan(0);
	});
});
