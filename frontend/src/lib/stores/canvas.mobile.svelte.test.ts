import { beforeEach, describe, expect, it } from 'vitest';
import type { ThreadDto } from '$lib/api/commands';
import {
	canvas,
	mobileCanvas,
	setLaneIndex,
	nextLane,
	prevLane,
	setMobilePxPerMs,
	MIN_MOBILE_PX_PER_MS,
	MAX_MOBILE_PX_PER_MS,
	LOD_THRESHOLD_PX_PER_MS,
	MOBILE_PX_PER_DAY
} from './canvas.svelte';

const MS_PER_DAY = 86_400_000;

function makeThread(i: number): ThreadDto {
	return { id: `t:${i}`, name: `Lane ${i}`, description: '', created_at: '2026-01-01T00:00:00Z' };
}

function setThreads(n: number) {
	canvas.threads = Array.from({ length: n }, (_, i) => makeThread(i));
}

beforeEach(() => {
	canvas.threads = [];
	mobileCanvas.currentLaneIndex = 0;
	mobileCanvas.pxPerMs = MOBILE_PX_PER_DAY / MS_PER_DAY;
});

// ── setLaneIndex ──────────────────────────────────────────────────────────────
describe('setLaneIndex', () => {
	it('sets the lane index to the given value within bounds', () => {
		setThreads(5);
		setLaneIndex(3);
		expect(mobileCanvas.currentLaneIndex).toBe(3);
	});

	it('clamps to 0 when given a negative value', () => {
		setThreads(5);
		setLaneIndex(-1);
		expect(mobileCanvas.currentLaneIndex).toBe(0);
	});

	it('clamps to the last index when value exceeds thread count', () => {
		setThreads(5); // indices 0-4
		setLaneIndex(99);
		expect(mobileCanvas.currentLaneIndex).toBe(4);
	});

	it('clamps to 0 when there are no threads', () => {
		canvas.threads = [];
		setLaneIndex(3);
		expect(mobileCanvas.currentLaneIndex).toBe(0);
	});

	it('clamps to 0 for a single-thread workspace (first = last)', () => {
		setThreads(1);
		setLaneIndex(5);
		expect(mobileCanvas.currentLaneIndex).toBe(0);
	});

	it('accepts index 0 explicitly', () => {
		setThreads(3);
		mobileCanvas.currentLaneIndex = 2;
		setLaneIndex(0);
		expect(mobileCanvas.currentLaneIndex).toBe(0);
	});

	it('accepts the exact last valid index', () => {
		setThreads(4); // last = 3
		setLaneIndex(3);
		expect(mobileCanvas.currentLaneIndex).toBe(3);
	});
});

// ── nextLane ──────────────────────────────────────────────────────────────────
describe('nextLane', () => {
	it('increments the index by 1', () => {
		setThreads(4);
		mobileCanvas.currentLaneIndex = 1;
		nextLane();
		expect(mobileCanvas.currentLaneIndex).toBe(2);
	});

	it('does not go past the last lane', () => {
		setThreads(3); // last = 2
		mobileCanvas.currentLaneIndex = 2;
		nextLane();
		expect(mobileCanvas.currentLaneIndex).toBe(2);
	});

	it('advances from index 0 to 1', () => {
		setThreads(3);
		mobileCanvas.currentLaneIndex = 0;
		nextLane();
		expect(mobileCanvas.currentLaneIndex).toBe(1);
	});

	it('calling nextLane n-1 times walks to the last lane', () => {
		setThreads(5);
		mobileCanvas.currentLaneIndex = 0;
		for (let i = 0; i < 4; i++) nextLane();
		expect(mobileCanvas.currentLaneIndex).toBe(4);
	});

	it('calling nextLane past the end stays at the last lane', () => {
		setThreads(3);
		mobileCanvas.currentLaneIndex = 0;
		for (let i = 0; i < 10; i++) nextLane();
		expect(mobileCanvas.currentLaneIndex).toBe(2);
	});
});

// ── prevLane ──────────────────────────────────────────────────────────────────
describe('prevLane', () => {
	it('decrements the index by 1', () => {
		setThreads(4);
		mobileCanvas.currentLaneIndex = 3;
		prevLane();
		expect(mobileCanvas.currentLaneIndex).toBe(2);
	});

	it('does not go below 0', () => {
		setThreads(3);
		mobileCanvas.currentLaneIndex = 0;
		prevLane();
		expect(mobileCanvas.currentLaneIndex).toBe(0);
	});

	it('calling prevLane past the start stays at 0', () => {
		setThreads(3);
		mobileCanvas.currentLaneIndex = 1;
		for (let i = 0; i < 10; i++) prevLane();
		expect(mobileCanvas.currentLaneIndex).toBe(0);
	});

	it('nextLane → prevLane round-trips back to origin', () => {
		setThreads(5);
		mobileCanvas.currentLaneIndex = 2;
		nextLane();
		prevLane();
		expect(mobileCanvas.currentLaneIndex).toBe(2);
	});

	it('prevLane → nextLane round-trips back to origin', () => {
		setThreads(5);
		mobileCanvas.currentLaneIndex = 2;
		prevLane();
		nextLane();
		expect(mobileCanvas.currentLaneIndex).toBe(2);
	});
});

// ── setMobilePxPerMs ──────────────────────────────────────────────────────────
describe('setMobilePxPerMs', () => {
	it('sets a value within the valid range', () => {
		const mid = (MIN_MOBILE_PX_PER_MS + MAX_MOBILE_PX_PER_MS) / 2;
		setMobilePxPerMs(mid);
		expect(mobileCanvas.pxPerMs).toBeCloseTo(mid, 15);
	});

	it('clamps to MIN_MOBILE_PX_PER_MS when value is below the floor', () => {
		setMobilePxPerMs(0);
		expect(mobileCanvas.pxPerMs).toBe(MIN_MOBILE_PX_PER_MS);
	});

	it('clamps to MIN when given a negative value', () => {
		setMobilePxPerMs(-1);
		expect(mobileCanvas.pxPerMs).toBe(MIN_MOBILE_PX_PER_MS);
	});

	it('clamps to MAX_MOBILE_PX_PER_MS when value exceeds the ceiling', () => {
		setMobilePxPerMs(Infinity);
		expect(mobileCanvas.pxPerMs).toBe(MAX_MOBILE_PX_PER_MS);
	});

	it('clamps to MAX when given a very large value', () => {
		setMobilePxPerMs(1e10);
		expect(mobileCanvas.pxPerMs).toBe(MAX_MOBILE_PX_PER_MS);
	});

	it('accepts the exact MIN boundary without clamping', () => {
		setMobilePxPerMs(MIN_MOBILE_PX_PER_MS);
		expect(mobileCanvas.pxPerMs).toBe(MIN_MOBILE_PX_PER_MS);
	});

	it('accepts the exact MAX boundary without clamping', () => {
		setMobilePxPerMs(MAX_MOBILE_PX_PER_MS);
		expect(mobileCanvas.pxPerMs).toBe(MAX_MOBILE_PX_PER_MS);
	});
});

// ── LOD constants — ordering invariants ───────────────────────────────────────
describe('LOD constants', () => {
	const defaultPxPerMs = MOBILE_PX_PER_DAY / MS_PER_DAY;

	it('MIN < LOD_THRESHOLD < default < MAX (strict ordering)', () => {
		expect(MIN_MOBILE_PX_PER_MS).toBeLessThan(LOD_THRESHOLD_PX_PER_MS);
		expect(LOD_THRESHOLD_PX_PER_MS).toBeLessThan(defaultPxPerMs);
		expect(defaultPxPerMs).toBeLessThan(MAX_MOBILE_PX_PER_MS);
	});

	it('default pxPerMs is within [MIN, MAX]', () => {
		expect(defaultPxPerMs).toBeGreaterThanOrEqual(MIN_MOBILE_PX_PER_MS);
		expect(defaultPxPerMs).toBeLessThanOrEqual(MAX_MOBILE_PX_PER_MS);
	});

	it('LOD_THRESHOLD is below the default — detail view is the startup state', () => {
		expect(LOD_THRESHOLD_PX_PER_MS).toBeLessThan(defaultPxPerMs);
	});

	it('all constants are positive', () => {
		expect(MIN_MOBILE_PX_PER_MS).toBeGreaterThan(0);
		expect(MAX_MOBILE_PX_PER_MS).toBeGreaterThan(0);
		expect(LOD_THRESHOLD_PX_PER_MS).toBeGreaterThan(0);
		expect(MOBILE_PX_PER_DAY).toBeGreaterThan(0);
	});

	it('MAX represents a useful zoom range (at least 10× the MIN)', () => {
		expect(MAX_MOBILE_PX_PER_MS / MIN_MOBILE_PX_PER_MS).toBeGreaterThanOrEqual(10);
	});
});
