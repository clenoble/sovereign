/** S5-8 on the phone — one click and Undo (C-DOC adoption step 5; spec ch. 03,
 *  *Soft delete and 30-day purge*, decided 2026-10-17: "Deleting a document
 *  is one click; the notice says it is in the trash and can be restored for
 *  30 days, and offers Undo"). Written before the code by another author. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mockTauriCommand } from '$lib/test/tauri';
import { undo, UNDO_WINDOW_MS, deleteWithUndo, undoAvailable, undoDelete } from './deleteUndo.svelte';

const T0 = 1_000_000;

beforeEach(() => {
	undo.notice = null;
	undo.error = null;
});

describe('one-click delete', () => {
	it('moves the document to the trash in one call and shows the notice', async () => {
		const deleted = vi.fn();
		mockTauriCommand('delete_document', (args: { id: string }) => deleted(args.id));
		await deleteWithUndo('document:a', T0);
		expect(deleted).toHaveBeenCalledTimes(1);
		expect(deleted).toHaveBeenCalledWith('document:a');
		expect(undo.notice?.docId).toBe('document:a');
		const text = undo.notice?.text ?? '';
		expect(text.toLowerCase()).toContain('trash');
		expect(text).toContain('30 days');
	});

	it('keeps the Undo window at 10 seconds ([choice])', () => {
		expect(UNDO_WINDOW_MS).toBe(10_000);
	});
});

describe('Undo', () => {
	beforeEach(() => {
		mockTauriCommand('delete_document', () => undefined);
	});

	it('restores the document while the notice is shown', async () => {
		const restored = vi.fn();
		mockTauriCommand('restore_from_trash', (args: { id: string }) => restored(args.id));
		await deleteWithUndo('document:a', T0);
		expect(undoAvailable(T0 + 3_000)).toBe(true);
		expect(await undoDelete(T0 + 3_000)).toBe(true);
		expect(restored).toHaveBeenCalledWith('document:a');
		expect(undo.notice).toBeNull();
	});

	it('is gone after the window: nothing is restored', async () => {
		const restored = vi.fn();
		mockTauriCommand('restore_from_trash', (args: unknown) => restored(args));
		await deleteWithUndo('document:a', T0);
		expect(undoAvailable(T0 + 9_000)).toBe(true);
		expect(undoAvailable(T0 + 11_000)).toBe(false);
		expect(await undoDelete(T0 + 11_000)).toBe(false);
		expect(restored).not.toHaveBeenCalled();
	});

	it('restores nothing when the document’s thread was deleted meanwhile, and says the thread is gone', async () => {
		// Céline's rule: nothing comes back from the trash into the fallback
		// lane. Undo asks for a plain restore (no thread chosen); the host
		// refuses it for a document whose thread is gone (sovereign-app,
		// s5_9_restore_from_trash_when_the_thread_is_gone_needs_a_thread_and_says_so).
		const calls: (string | null)[] = [];
		mockTauriCommand('restore_from_trash', (args: { id: string; threadId?: string | null }) => {
			calls.push(args.threadId ?? null);
			throw new Error('This document’s thread was deleted: choose a thread to restore it into.');
		});
		await deleteWithUndo('document:a', T0);
		expect(await undoDelete(T0 + 2_000)).toBe(false);
		expect(calls).toEqual([null]);
		expect(undo.error?.toLowerCase()).toContain('thread');
	});

	it('does nothing and says so when the document already left the trash', async () => {
		mockTauriCommand('restore_from_trash', () => {
			throw new Error('This document is not in the trash.');
		});
		await deleteWithUndo('document:a', T0);
		expect(await undoDelete(T0 + 2_000)).toBe(false);
		expect(undo.error).toBeTruthy();
	});
});
