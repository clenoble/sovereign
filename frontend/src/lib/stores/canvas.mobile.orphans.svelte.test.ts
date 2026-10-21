/** S5-12 W3 on the phone — never hidden (`doc/plans/C-DOC-adoption.md`, *S5-12*:
 *  "A listed document stays in every list and on the canvas (the fallback
 *  lane), readable and editable, until resolved"). Today the phone builds its
 *  lanes from active threads only, so such a document is not drawn at all
 *  (spec ch. 03, *Thread deletion and the fallback lane*, *As built*).
 *  Written before the code by another author. */
import { beforeEach, describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import type { CanvasDocDto, ThreadDto } from '$lib/api/commands';
import {
	canvas,
	mobileCanvas,
	mobileLanes,
	laneIdOf,
	setLaneIndex,
	FALLBACK_LANE_ID,
	FALLBACK_LANE_NAME
} from './canvas.svelte';

const thread = (id: string, name: string): ThreadDto => ({ id, name, description: '', created_at: '2026-01-01T00:00:00Z' });

const doc = (id: string, threadId: string): CanvasDocDto => ({
	id,
	title: `Doc ${id}`,
	thread_id: threadId,
	is_owned: true,
	spatial_x: 0,
	spatial_y: 0,
	created_at: '2026-10-01T00:00:00Z',
	modified_at: '2026-10-01T00:00:00Z',
	reliability_classification: null,
	reliability_score: null,
	source_url: null
});

beforeEach(() => {
	canvas.threads = [thread('thread:r', 'Research'), thread('thread:k', 'Keep')];
	canvas.documents = [];
	mobileCanvas.currentLaneIndex = 0;
});

describe('the phone draws a document of a deleted thread in the fallback lane', () => {
	it('adds the fallback lane after the threads when a document needs it', () => {
		canvas.documents = [doc('document:a', 'thread:r'), doc('document:orphan', 'thread:deleted-here')];
		const lanes = mobileLanes();
		expect(lanes.map((l) => l.id)).toEqual(['thread:r', 'thread:k', FALLBACK_LANE_ID]);
		expect(lanes.map((l) => l.fallback)).toEqual([false, false, true]);
		expect(lanes[2].name).toBe(FALLBACK_LANE_NAME);
		expect(lanes[0].name).toBe('Research');
	});

	it('draws the document there, and every other document in its own thread', () => {
		canvas.documents = [doc('document:a', 'thread:r'), doc('document:orphan', 'thread:deleted-here')];
		expect(laneIdOf(canvas.documents[1])).toBe(FALLBACK_LANE_ID);
		expect(laneIdOf(canvas.documents[0])).toBe('thread:r');
		// Every document is in exactly one lane that exists: none hidden.
		const ids = new Set(mobileLanes().map((l) => l.id));
		for (const d of canvas.documents) expect(ids.has(laneIdOf(d))).toBe(true);
	});

	it('shows no fallback lane when every document has its thread', () => {
		canvas.documents = [doc('document:a', 'thread:r'), doc('document:b', 'thread:k')];
		expect(mobileLanes().some((l) => l.fallback)).toBe(false);
		expect(mobileLanes().length).toBe(2);
	});

	it('lets the user swipe to the fallback lane', () => {
		canvas.documents = [doc('document:orphan', 'thread:deleted-here')];
		setLaneIndex(99);
		expect(mobileCanvas.currentLaneIndex).toBe(2);
		expect(mobileLanes()[mobileCanvas.currentLaneIndex].fallback).toBe(true);
	});

	it('MobileCanvas draws its lanes from mobileLanes()', () => {
		const src = readFileSync('src/lib/components/mobile/MobileCanvas.svelte', 'utf8');
		expect(src).toContain('mobileLanes(');
	});
});
