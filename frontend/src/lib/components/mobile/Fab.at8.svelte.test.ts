/** C-HOST AT8 on the phone (spec ch. 08, *Action targets*, AT8: "A menu, a list
 *  or a drag in any host carries the object's id, never its index in a list
 *  that a reload can reorder. This holds on the phone, which has no
 *  orchestrator").
 *
 *  The phone's "+" puts a new document in the lane the person is on. The person
 *  picks a lane in the lane switcher; then a reload (a sync round, another
 *  device creating a thread) adds a thread before it; then the person taps
 *  "+". The document must go in the lane picked, never in whichever thread now
 *  holds that lane's old position.
 *
 *  Written by the LH test author (2026-11-14, PR-0011); the implementer does not
 *  edit this file. Red against fae2d23: the lane is held by its index
 *  (`mobileCanvas.currentLaneIndex`, read by `Fab.svelte` as
 *  `canvas.threads[currentLaneIndex]`). */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { ThreadDto } from '$lib/api/commands';

const h = vi.hoisted(() => ({
	created: [] as Array<[string, string]>,
	threads: [] as unknown[]
}));

vi.mock('$lib/api/commands', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api/commands')>();
	return {
		...actual,
		createDocument: vi.fn(async (title: string, threadId: string) => {
			h.created.push([title, threadId]);
			return 'document:new';
		}),
		canvasLoad: vi.fn(async () => ({ documents: [], threads: h.threads, relationships: [], milestones: [] }))
	};
});

import Fab from './Fab.svelte';
import LaneSwitcherSheet from './LaneSwitcherSheet.svelte';
import { canvas, mobileCanvas } from '$lib/stores/canvas.svelte';

const thread = (id: string, name: string): ThreadDto => ({ id, name, description: '', created_at: '2026-01-01T00:00:00Z' });

const GARDEN = thread('thread:g', 'Garden');
const KITCHEN = thread('thread:k', 'Kitchen');
const ATTIC = thread('thread:a', 'Attic');

beforeEach(() => {
	h.created.length = 0;
	canvas.threads = [GARDEN, KITCHEN];
	canvas.documents = [];
	mobileCanvas.currentLaneIndex = 0;
	window.prompt = vi.fn(() => 'Minutes');
	window.alert = vi.fn();
});

describe('AT8 on the phone: the "+" goes in the lane picked, by its id', () => {
	it('after a reload adds a thread before the lane picked, the new document goes in that lane', async () => {
		// The person picks "Kitchen" in the lane switcher.
		const sheet = render(LaneSwitcherSheet, { props: { open: true } });
		const row = sheet.getByText('Kitchen').closest('button');
		expect(row).not.toBeNull();
		await fireEvent.click(row!);
		sheet.unmount();

		// A reload: a thread synced from another device now comes first.
		canvas.threads = [ATTIC, GARDEN, KITCHEN];
		h.threads = canvas.threads;
		await tick();

		// The person taps "+" and names the document.
		const fab = render(Fab);
		const plus = fab.container.querySelector('button.fab');
		expect(plus).not.toBeNull();
		await fireEvent.click(plus!);
		await waitFor(() => expect(h.created.length).toBe(1));

		expect(h.created[0]).toEqual(['Minutes', KITCHEN.id]);
	});
});
