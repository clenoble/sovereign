/** v0.0.12 item 9 on the phone's trash panel
 *  (`doc/plans/V0012-item9-act-on-what-was-shown.md`). Written before the code
 *  by another author; the implementer does not edit it.
 *
 *  - T1: the confirmation sends the ids shown (`empty_trash { ids }`), and
 *    "[choice] The confirm's message names the skipped ones when there are
 *    any" — settled (contract cb5a859): a shown document no longer in the
 *    trash (restored or purged elsewhere) is skipped, and the message is
 *    neutral: "1 document was no longer in the trash and was left alone."
 *    The host answers the number purged; a shown document not purged was
 *    skipped.
 *  - T4 (a guard, green today): "Restore into…" offers every live thread, in
 *    the lanes' order (the phone's chooser is a native select). */
import { beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, within } from '@testing-library/svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { canvas } from '$lib/stores/canvas.svelte';
import { trash } from '$lib/stores/trash.svelte';
import TrashPanel from './TrashPanel.svelte';

const dto = (id: string, title: string, threadId: string, threadName: string | null) => ({
	id,
	title,
	thread_id: threadId,
	thread_name: threadName,
	deleted_thread_name: threadName === null ? 'Old notes' : null,
	trashed_at: '2026-10-20T10:00:00Z',
	days_left: 26
});

const threads = Array.from({ length: 30 }, (_, i) => ({
	id: `thread:t${i}`,
	name: `Thread ${String(i).padStart(3, '0')}`,
	description: '',
	created_at: `2026-01-01T00:00:${String(i).padStart(2, '0')}Z`
}));

let listing: ReturnType<typeof dto>[] = [];

beforeEach(() => {
	trash.lines = [];
	trash.emptyRequested = false;
	trash.error = null;
	canvas.threads = threads as typeof canvas.threads;
	mockTauriCommand('list_threads', () => threads);
	listing = [dto('document:b', 'Beta', 'thread:t1', 'Thread 001'), dto('document:a', 'Alpha', 'thread:t2', 'Thread 002')];
	mockTauriCommand('list_trash', () => listing);
});

async function confirmEmptyFromPanel() {
	const r = render(TrashPanel);
	await fireEvent.click(await r.findByRole('button', { name: /empty trash/i }));
	const dialog = await r.findByRole('dialog');
	return { ...r, dialog };
}

describe('TrashPanel — Empty trash acts on what was shown (T1)', () => {
	it('sends the shown ids, though a peer’s deletion arrived meanwhile', async () => {
		let sent: string[] | undefined;
		mockTauriCommand('empty_trash', (a: { ids?: string[] }) => {
			sent = a?.ids;
			listing = [dto('document:g', 'Gamma', 'thread:t1', 'Thread 001')];
			return 2;
		});
		const { dialog, findByText } = await confirmEmptyFromPanel();
		listing = [dto('document:g', 'Gamma', 'thread:t1', 'Thread 001'), ...listing]; // the peer's, unseen
		await fireEvent.click(within(dialog).getByRole('button', { name: /^empty/i }));
		expect([...(sent ?? [])].sort()).toEqual(['document:a', 'document:b']);
		expect(await findByText('Gamma'), 'the next view lists the peer’s document').toBeTruthy();
	});

	it('names a shown document no longer in the trash, left alone', async () => {
		mockTauriCommand('empty_trash', () => {
			listing = [];
			return 1; // Alpha was restored meanwhile: only Beta was purged
		});
		const { dialog, container } = await confirmEmptyFromPanel();
		await fireEvent.click(within(dialog).getByRole('button', { name: /^empty/i }));
		await new Promise((r) => setTimeout(r, 0));
		const status = Array.from(container.querySelectorAll('[role="status"]')).map((e) => e.textContent ?? '').join(' ');
		expect(status).toContain('1 document was no longer in the trash and was left alone.');
	});

	it('names nothing kept when everything shown was purged', async () => {
		mockTauriCommand('empty_trash', () => {
			listing = [];
			return 2;
		});
		const { dialog, container } = await confirmEmptyFromPanel();
		await fireEvent.click(within(dialog).getByRole('button', { name: /^empty/i }));
		await new Promise((r) => setTimeout(r, 0));
		expect(container.textContent ?? '').not.toContain('left alone');
	});
});

describe('TrashPanel — “Restore into…” offers every thread (T4, a guard)', () => {
	it('lists all 30 live threads, in the lanes’ order', async () => {
		listing = [dto('document:b', 'Beta', 'thread:gone', null)];
		const { findByRole } = render(TrashPanel);
		await fireEvent.click(await findByRole('button', { name: 'Restore Beta' }));
		const dialog = await findByRole('dialog');
		const select = within(dialog).getByRole('combobox', { name: /thread/i }) as HTMLSelectElement;
		expect(Array.from(select.options).map((o) => o.textContent?.trim())).toEqual(threads.map((t) => t.name));
	});
});
