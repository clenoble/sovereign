/** v0.0.12 item 9, T3 on the phone — Delete says what it did
 *  (`doc/plans/V0012-item9-act-on-what-was-shown.md`). Written before the code
 *  by another author; the implementer does not edit it.
 *
 *  "`trash` tells 'moved to the trash' apart from 'was already in the trash'.
 *  In the second case, the desktop and the phone say 'Already in the trash',
 *  with no Undo. An Undo restores only a deletion that this Delete made."
 *
 *  Seam named (not there at `c7e571b`): `delete_document` answers
 *  `'trashed'` or `'already_in_trash'` (the host's `DeleteOutcome`, snake
 *  case). What the phone shows is read from `UndoToast`, as the user meets it;
 *  where the store keeps it is not assumed. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { app } from '$lib/stores/app.svelte';
import { undo, deleteWithUndo, undoAvailable, undoDelete } from './deleteUndo.svelte';
import UndoToast from '$lib/components/UndoToast.svelte';
import ContextMenu from '$lib/components/ContextMenu.svelte';

const T0 = 1_000_000;

beforeEach(() => {
	undo.notice = null;
	undo.error = null;
	app.contextMenu = null;
});

describe('Delete of a document already in the trash (T3)', () => {
	it('says “Already in the trash”, with no Undo', async () => {
		mockTauriCommand('delete_document', () => 'already_in_trash');
		const { container, queryByRole } = render(UndoToast);
		await deleteWithUndo('document:a');
		await tick();
		expect(container.textContent ?? '').toContain('Already in the trash');
		expect(queryByRole('button', { name: 'Undo' }), 'no Undo').toBeNull();
		expect(container.textContent ?? '').not.toContain('30 days');
	});

	it('offers no Undo, and an Undo restores nothing', async () => {
		const restored = vi.fn();
		mockTauriCommand('delete_document', () => 'already_in_trash');
		mockTauriCommand('restore_from_trash', (a: unknown) => restored(a));
		await deleteWithUndo('document:a', T0);
		expect(undoAvailable(T0 + 1_000)).toBe(false);
		expect(await undoDelete(T0 + 1_000)).toBe(false);
		expect(restored, 'the deletion was not this Delete’s').not.toHaveBeenCalled();
	});

	it('from the context menu too', async () => {
		mockTauriCommand('delete_document', () => 'already_in_trash');
		app.contextMenu = { x: 10, y: 10, docId: 'document:a', threadId: 'thread:t' };
		const menu = render(ContextMenu);
		const toast = render(UndoToast);
		await fireEvent.click(menu.getByRole('menuitem', { name: 'Delete' }));
		await tick();
		await tick();
		expect(toast.container.textContent ?? '').toContain('Already in the trash');
		expect(toast.queryByRole('button', { name: 'Undo' })).toBeNull();
	});
});

describe('Delete that moved the document (T3, the first case)', () => {
	it('offers Undo, which restores that document', async () => {
		const restored = vi.fn();
		mockTauriCommand('delete_document', () => 'trashed');
		mockTauriCommand('restore_from_trash', (a: { id: string }) => restored(a.id));
		const { getByRole, container } = render(UndoToast);
		await deleteWithUndo('document:a');
		await tick();
		expect(container.textContent ?? '').toContain('30 days');
		await fireEvent.click(getByRole('button', { name: 'Undo' }));
		await tick();
		expect(restored).toHaveBeenCalledWith('document:a');
	});
});
