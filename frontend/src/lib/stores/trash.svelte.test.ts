/** S5-11 / S5-5..S5-7 on the phone — the trash store (C-DOC adoption step 5;
 *  spec ch. 03, *Soft delete and 30-day purge*, decided 2026-10-17).
 *  Written before the code by another author: whoever implements step 5 makes
 *  these pass without editing this file. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mockTauriCommand } from '$lib/test/tauri';
import {
	trash,
	loadTrash,
	threadLabel,
	restore,
	deletedThreadGroups,
	restoreGroup,
	requestEmpty,
	cancelEmpty,
	confirmEmpty
} from './trash.svelte';

/** The old names of the deleted threads, as the host sends them
 *  (`deleted_thread_name`, beside `thread_name: null`). */
const OLD_NAMES: Record<string, string> = { 'thread:gone': 'Old notes', 'thread:x': 'X', 'thread:y': 'Y' };

const dto = (id: string, title: string, threadId: string, threadName: string | null, daysLeft: number) => ({
	id,
	title,
	thread_id: threadId,
	thread_name: threadName,
	deleted_thread_name: threadName === null ? (OLD_NAMES[threadId] ?? null) : null,
	trashed_at: '2026-10-07T10:00:00Z',
	days_left: daysLeft
});

let listing: ReturnType<typeof dto>[] = [];

beforeEach(() => {
	trash.lines = [];
	trash.emptyRequested = false;
	trash.error = null;
	listing = [dto('document:b', 'Beta', 'thread:gone', null, 25), dto('document:a', 'Alpha', 'thread:r', 'Research', 20)];
	mockTauriCommand('list_trash', () => listing);
});

describe('trash — the list', () => {
	it('reads the trash newest first, with thread and days left', async () => {
		await loadTrash();
		expect(trash.lines.map((l) => [l.id, l.title, l.threadId, l.threadName, l.daysLeft])).toEqual([
			['document:b', 'Beta', 'thread:gone', null, 25],
			['document:a', 'Alpha', 'thread:r', 'Research', 20]
		]);
	});

	it('names a live thread, and says when the thread was deleted — never the fallback lane', async () => {
		await loadTrash();
		expect(threadLabel(trash.lines[1])).toBe('Research');
		// Céline (2026-10-17): the thread's old name, then " - deleted".
		const gone = threadLabel(trash.lines[0]);
		expect(gone).toBe('Old notes - deleted');
		expect(gone).not.toContain('Unfiled');
	});
});

describe('trash — restore', () => {
	it('restores into its own thread when the thread exists, and the list follows at once', async () => {
		const restored = vi.fn();
		mockTauriCommand('restore_from_trash', (args: { id: string; threadId?: string | null }) => {
			restored(args.id, args.threadId ?? null);
			listing = listing.filter((l) => l.id !== args.id);
		});
		await loadTrash();
		expect(await restore('document:a')).toBe(true);
		expect(restored).toHaveBeenCalledWith('document:a', null);
		expect(trash.lines.map((l) => l.id)).toEqual(['document:b']);
	});

	it('does not restore a document whose thread was deleted without a chosen thread, and says why', async () => {
		const restored = vi.fn();
		mockTauriCommand('restore_from_trash', (args: unknown) => restored(args));
		await loadTrash();
		expect(await restore('document:b')).toBe(false);
		expect(restored).not.toHaveBeenCalled();
		expect(trash.error?.toLowerCase()).toContain('thread');
		expect(trash.lines.map((l) => l.id)).toContain('document:b');
	});

	it('restores a document whose thread was deleted into the chosen thread', async () => {
		const restored = vi.fn();
		mockTauriCommand('restore_from_trash', (args: { id: string; threadId?: string | null }) => {
			restored(args.id, args.threadId ?? null);
			listing = listing.filter((l) => l.id !== args.id);
		});
		await loadTrash();
		expect(await restore('document:b', 'thread:new')).toBe(true);
		expect(restored).toHaveBeenCalledWith('document:b', 'thread:new');
		expect(trash.lines.map((l) => l.id)).toEqual(['document:a']);
	});
});

describe('trash — grouped by deleted thread (Céline, 2026-10-17)', () => {
	beforeEach(() => {
		listing = [
			dto('document:x2', 'X two', 'thread:x', null, 25),
			dto('document:y1', 'Y one', 'thread:y', null, 24),
			dto('document:x1', 'X one', 'thread:x', null, 23),
			dto('document:a', 'Alpha', 'thread:r', 'Research', 20)
		];
	});

	it('groups the documents of each deleted thread; a live thread’s document is in no group', async () => {
		await loadTrash();
		const groups = deletedThreadGroups()
			.map((g) => [g.threadId, [...g.ids].sort()] as const)
			.sort((a, b) => a[0].localeCompare(b[0]));
		expect(groups).toEqual([
			['thread:x', ['document:x1', 'document:x2']],
			['thread:y', ['document:y1']]
		]);
		const labels = Object.fromEntries(deletedThreadGroups().map((g) => [g.threadId, g.label]));
		expect(labels).toEqual({ 'thread:x': 'X - deleted', 'thread:y': 'Y - deleted' });
	});

	it('restores every document of a group into the chosen thread, and the list follows', async () => {
		const restored: [string, string | null][] = [];
		mockTauriCommand('restore_from_trash', (args: { id: string; threadId?: string | null }) => {
			restored.push([args.id, args.threadId ?? null]);
			listing = listing.filter((l) => l.id !== args.id);
		});
		await loadTrash();
		expect(await restoreGroup('thread:x', 'thread:new')).toBe(true);
		expect([...restored].sort()).toEqual([
			['document:x1', 'thread:new'],
			['document:x2', 'thread:new']
		]);
		expect(trash.lines.map((l) => l.id).sort()).toEqual(['document:a', 'document:y1']);
	});

	it('one document of a group can still be restored alone', async () => {
		const restored = vi.fn();
		mockTauriCommand('restore_from_trash', (args: { id: string; threadId?: string | null }) => {
			restored(args.id, args.threadId ?? null);
			listing = listing.filter((l) => l.id !== args.id);
		});
		await loadTrash();
		expect(await restore('document:x1', 'thread:new')).toBe(true);
		expect(restored).toHaveBeenCalledTimes(1);
		expect(deletedThreadGroups().find((g) => g.threadId === 'thread:x')?.ids).toEqual(['document:x2']);
	});

	it('a group restore into a thread that does not exist is refused, and nothing is restored', async () => {
		mockTauriCommand('restore_from_trash', () => {
			throw new Error('That thread does not exist.');
		});
		await loadTrash();
		expect(await restoreGroup('thread:x', 'thread:nope')).toBe(false);
		expect(trash.error).toBeTruthy();
		expect(trash.lines.map((l) => l.id).sort()).toEqual(['document:a', 'document:x1', 'document:x2', 'document:y1']);
	});
});

describe('trash — empty', () => {
	it('asks first: requesting purges nothing, cancelling purges nothing', async () => {
		const emptied = vi.fn(() => 2);
		mockTauriCommand('empty_trash', emptied);
		await loadTrash();
		requestEmpty();
		expect(trash.emptyRequested).toBe(true);
		expect(emptied).not.toHaveBeenCalled();
		cancelEmpty();
		expect(trash.emptyRequested).toBe(false);
		expect(await confirmEmpty()).toBeNull();
		expect(emptied).not.toHaveBeenCalled();
	});

	it('empties after the confirmation and reports how many were purged', async () => {
		const emptied = vi.fn(() => 2);
		mockTauriCommand('empty_trash', emptied);
		await loadTrash();
		requestEmpty();
		listing = [];
		expect(await confirmEmpty()).toBe(2);
		expect(emptied).toHaveBeenCalledTimes(1);
		expect(trash.lines).toEqual([]);
		expect(trash.emptyRequested).toBe(false);
	});
});
