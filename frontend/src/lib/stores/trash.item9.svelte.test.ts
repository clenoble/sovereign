/** v0.0.12 item 9, T1 on the phone — Empty trash empties what was shown
 *  (`doc/plans/V0012-item9-act-on-what-was-shown.md`). Written before the code
 *  by another author; the implementer does not edit it.
 *
 *  Seams named (not there at `c7e571b`):
 *  - `empty_trash` is invoked with `{ ids }`: the ids the trash view listed
 *    when it was drawn (`trash.lines`), never re-read at the confirm. It still
 *    answers the number purged.
 *  - `confirmEmpty()` still resolves to that number (or `null` without a
 *    confirmation). */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mockTauriCommand } from '$lib/test/tauri';
import { trash, loadTrash, requestEmpty, confirmEmpty } from './trash.svelte';

const dto = (id: string, title: string) => ({
	id,
	title,
	thread_id: 'thread:r',
	thread_name: 'Research',
	deleted_thread_name: null,
	trashed_at: '2026-10-20T10:00:00Z',
	days_left: 26
});

/** What the host lists now. */
let listing: ReturnType<typeof dto>[] = [];

beforeEach(() => {
	trash.lines = [];
	trash.emptyRequested = false;
	trash.error = null;
	listing = [dto('document:b', 'Beta'), dto('document:a', 'Alpha')];
	mockTauriCommand('list_trash', () => listing);
});

describe('Empty trash on the phone acts on what was shown (T1)', () => {
	it('sends the ids the view listed when it was drawn, not what the trash holds at the confirm', async () => {
		const calls: unknown[] = [];
		mockTauriCommand('empty_trash', (args: { ids?: string[] }) => {
			calls.push(args?.ids);
			return 2;
		});
		await loadTrash();
		requestEmpty();
		// Meanwhile a peer's deletion puts Gamma in the trash; the view was not redrawn.
		listing = [dto('document:g', 'Gamma'), ...listing];
		expect(await confirmEmpty()).toBe(2);
		expect(calls).toHaveLength(1);
		expect([...((calls[0] as string[]) ?? [])].sort()).toEqual(['document:a', 'document:b']);
	});

	it('lists, after the confirm, what the host still holds (the next view)', async () => {
		mockTauriCommand('empty_trash', () => {
			listing = [dto('document:g', 'Gamma')];
			return 2;
		});
		await loadTrash();
		requestEmpty();
		await confirmEmpty();
		expect(trash.lines.map((l) => l.id)).toEqual(['document:g']);
	});

	it('sends nothing without the confirmation (unchanged)', async () => {
		const emptied = vi.fn(() => 0);
		mockTauriCommand('empty_trash', emptied);
		await loadTrash();
		expect(await confirmEmpty()).toBeNull();
		expect(emptied).not.toHaveBeenCalled();
	});
});
