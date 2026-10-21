/** S5-11 — the trash in the phone's Settings (C-DOC adoption step 5; spec
 *  ch. 03, decided 2026-10-17: "The trash is reached from Settings"; emptying
 *  it asks first; a document whose thread was deleted goes back into a thread
 *  the user chooses). Written before the code by another author.
 *
 *  The DOM contract kept to what a person meets: each line's title, thread
 *  and days left; a "Restore <title>" button; for a deleted thread, a dialog
 *  to choose the thread (a select labelled with "thread", and a Restore
 *  button); "Empty trash", which opens a confirmation dialog. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, fireEvent, within } from '@testing-library/svelte';
import { readFileSync } from 'node:fs';
import { mockTauriCommand } from '$lib/test/tauri';
import { canvas } from '$lib/stores/canvas.svelte';
import { trash } from '$lib/stores/trash.svelte';
import TrashPanel from './TrashPanel.svelte';

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

const threads = [
	{ id: 'thread:r', name: 'Research', description: '', created_at: '2026-01-01T00:00:00Z' },
	{ id: 'thread:new', name: 'Brought back', description: '', created_at: '2026-10-17T00:00:00Z' }
];

beforeEach(() => {
	trash.lines = [];
	trash.emptyRequested = false;
	trash.error = null;
	canvas.threads = threads as typeof canvas.threads;
	mockTauriCommand('list_threads', () => threads);
	mockTauriCommand('list_trash', () => [
		dto('document:b', 'Beta', 'thread:gone', null, 25),
		dto('document:a', 'Alpha', 'thread:r', 'Research', 20)
	]);
});

describe('TrashPanel — the list', () => {
	it('lists each document with its thread and days left', async () => {
		const { findByText, container } = render(TrashPanel);
		await findByText('Alpha');
		const text = container.textContent ?? '';
		expect(text).toContain('Beta');
		expect(text).toContain('Research');
		expect(text).toContain('20 days');
		expect(text).toContain('25 days');
	});

	it('says a deleted thread was deleted, never the fallback lane', async () => {
		const { findByText, container } = render(TrashPanel);
		await findByText('Beta');
		// Céline (2026-10-17): the thread's old name, then " - deleted".
		expect(container.textContent ?? '').toContain('Old notes - deleted');
		expect(container.textContent ?? '').not.toContain('Unfiled');
	});
});

describe('TrashPanel — restore', () => {
	it('restores into its own thread in one click when the thread exists', async () => {
		const restored = vi.fn();
		mockTauriCommand('restore_from_trash', (args: { id: string; threadId?: string | null }) =>
			restored(args.id, args.threadId ?? null)
		);
		const { findByRole } = render(TrashPanel);
		await fireEvent.click(await findByRole('button', { name: 'Restore Alpha' }));
		expect(restored).toHaveBeenCalledWith('document:a', null);
	});

	it('asks for a thread when the document’s thread was deleted, then restores into it', async () => {
		const restored = vi.fn();
		mockTauriCommand('restore_from_trash', (args: { id: string; threadId?: string | null }) =>
			restored(args.id, args.threadId ?? null)
		);
		const { findByRole } = render(TrashPanel);
		await fireEvent.click(await findByRole('button', { name: 'Restore Beta' }));
		expect(restored).not.toHaveBeenCalled();
		const dialog = await findByRole('dialog');
		const select = within(dialog).getByRole('combobox', { name: /thread/i }) as HTMLSelectElement;
		const names = Array.from(select.options).map((o) => o.textContent?.trim());
		expect(names).toEqual(expect.arrayContaining(['Research', 'Brought back']));
		await fireEvent.change(select, { target: { value: 'thread:new' } });
		await fireEvent.click(within(dialog).getByRole('button', { name: /restore/i }));
		expect(restored).toHaveBeenCalledWith('document:b', 'thread:new');
	});
});

describe('TrashPanel — grouped by deleted thread (Céline, 2026-10-17)', () => {
	beforeEach(() => {
		mockTauriCommand('list_trash', () => [
			dto('document:b', 'Beta', 'thread:gone', null, 25),
			dto('document:g', 'Gamma', 'thread:gone', null, 24),
			dto('document:a', 'Alpha', 'thread:r', 'Research', 20)
		]);
	});

	it('“Restore all into a thread…” restores every document of the group into the chosen thread', async () => {
		const restored: [string, string | null][] = [];
		mockTauriCommand('restore_from_trash', (args: { id: string; threadId?: string | null }) => {
			restored.push([args.id, args.threadId ?? null]);
		});
		const { findAllByRole, findByRole } = render(TrashPanel);
		const buttons = await findAllByRole('button', { name: /restore all/i });
		expect(buttons.length).toBe(1);
		await fireEvent.click(buttons[0]);
		expect(restored).toEqual([]);
		const dialog = await findByRole('dialog');
		const select = within(dialog).getByRole('combobox', { name: /thread/i }) as HTMLSelectElement;
		await fireEvent.change(select, { target: { value: 'thread:new' } });
		await fireEvent.click(within(dialog).getByRole('button', { name: /restore/i }));
		await new Promise((r) => setTimeout(r, 0));
		expect([...restored].sort()).toEqual([
			['document:b', 'thread:new'],
			['document:g', 'thread:new']
		]);
	});

	it('a document of the group can still be restored alone', async () => {
		const { findByRole } = render(TrashPanel);
		expect(await findByRole('button', { name: 'Restore Gamma' })).toBeTruthy();
		expect(await findByRole('button', { name: 'Restore Beta' })).toBeTruthy();
	});
});

describe('TrashPanel — empty', () => {
	it('asks before emptying, and empties only on the confirmation', async () => {
		const emptied = vi.fn(() => 2);
		mockTauriCommand('empty_trash', emptied);
		const { findByRole, queryByRole } = render(TrashPanel);
		await fireEvent.click(await findByRole('button', { name: /empty trash/i }));
		const dialog = await findByRole('dialog');
		expect(emptied).not.toHaveBeenCalled();
		await fireEvent.click(within(dialog).getByRole('button', { name: /cancel/i }));
		expect(emptied).not.toHaveBeenCalled();
		expect(queryByRole('dialog')).toBeNull();

		await fireEvent.click(await findByRole('button', { name: /empty trash/i }));
		const again = await findByRole('dialog');
		await fireEvent.click(within(again).getByRole('button', { name: /^empty/i }));
		expect(emptied).toHaveBeenCalledTimes(1);
	});
});

describe('TrashPanel — placement', () => {
	it('is part of the Settings panel', () => {
		const settings = readFileSync('src/lib/components/SettingsPanel.svelte', 'utf8');
		expect(settings).toMatch(/import\s+TrashPanel\s+from/);
		expect(settings).toContain('<TrashPanel');
	});
});
