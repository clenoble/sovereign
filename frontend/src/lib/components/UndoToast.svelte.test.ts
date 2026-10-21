/** S5-8 / S5-11 — the one-click delete and its Undo notice on the phone (C-DOC
 *  adoption step 5; spec ch. 03, decided 2026-10-17). Written before the code
 *  by another author. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { app } from '$lib/stores/app.svelte';
import { undo, deleteWithUndo } from '$lib/stores/deleteUndo.svelte';
import UndoToast from './UndoToast.svelte';
import ContextMenu from './ContextMenu.svelte';

beforeEach(() => {
	undo.notice = null;
	undo.error = null;
	app.contextMenu = null;
});

describe('ContextMenu — Delete is one click', () => {
	it('moves the document to the trash on the first click, with no confirmation step', async () => {
		const deleted = vi.fn();
		mockTauriCommand('delete_document', (args: { id: string }) => deleted(args.id));
		app.contextMenu = { x: 10, y: 10, docId: 'document:a', threadId: 'thread:t' };
		const { getByRole, container } = render(ContextMenu);
		await fireEvent.click(getByRole('menuitem', { name: 'Delete' }));
		await tick();
		expect(deleted).toHaveBeenCalledTimes(1);
		expect(deleted).toHaveBeenCalledWith('document:a');
		expect(undo.notice?.docId).toBe('document:a');
		expect(container.textContent ?? '').not.toMatch(/confirm/i);
	});
});

describe('UndoToast', () => {
	beforeEach(() => {
		mockTauriCommand('delete_document', () => undefined);
	});

	it('says the document is in the trash for 30 days, with Undo', async () => {
		const { container, getByRole } = render(UndoToast);
		await deleteWithUndo('document:a');
		await tick();
		const status = container.querySelector('[role="status"]');
		expect(status?.textContent?.toLowerCase()).toContain('trash');
		expect(status?.textContent).toContain('30 days');
		expect(getByRole('button', { name: 'Undo' })).toBeTruthy();
	});

	it('Undo restores the document', async () => {
		const restored = vi.fn();
		mockTauriCommand('restore_from_trash', (args: { id: string }) => restored(args.id));
		const { getByRole } = render(UndoToast);
		await deleteWithUndo('document:a');
		await tick();
		await fireEvent.click(getByRole('button', { name: 'Undo' }));
		await tick();
		expect(restored).toHaveBeenCalledWith('document:a');
	});

	it('shows nothing when there is no notice', () => {
		const { container } = render(UndoToast);
		expect(container.querySelector('[role="status"]')).toBeNull();
	});
});
