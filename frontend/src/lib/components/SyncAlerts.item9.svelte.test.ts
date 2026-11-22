/** v0.0.12 item 9, T2 on the phone — an orphan group's action acts on the
 *  documents shown (`doc/plans/V0012-item9-act-on-what-was-shown.md`).
 *  Written before the code by another author; the implementer does not edit
 *  it.
 *
 *  "'Move all to a thread…', 'All in a new thread…' and 'Trash all' apply to
 *  the ids the group listed when it was drawn, those of them still waiting. A
 *  document that joined the group after is left waiting, and is listed at the
 *  next refresh."
 *
 *  Seam named (not there at `c7e571b`): `resolve_orphans_of_thread` is
 *  invoked with `{ threadId, docIds, resolution }`, `docIds` the group's
 *  documents as drawn. The DOM is the one `SyncAlerts.orphans.svelte.test.ts`
 *  fixes (the mobile sheet, a group per deleted thread). */
import { beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { canvas } from '$lib/stores/canvas.svelte';
import SyncAlerts from './SyncAlerts.svelte';
import { syncAlerts, reloadSyncAlerts } from '$lib/stores/syncAlerts.svelte';

const orphanDto = (id: string, title: string, threadId: string, old: string | null) => ({
	doc_id: id,
	title,
	thread_id: threadId,
	deleted_thread_name: old,
	device: `peer-${id}`,
	device_name: 'Laptop'
});

const threads = [
	{ id: 'thread:k', name: 'Keep', description: '', created_at: '2026-01-01T00:00:00Z' },
	{ id: 'thread:r', name: 'Research', description: '', created_at: '2026-01-02T00:00:00Z' }
];

let waiting: ReturnType<typeof orphanDto>[] = [];

beforeEach(() => {
	syncAlerts.alerts = [];
	syncAlerts.previousValues = [];
	syncAlerts.olderDevices = [];
	syncAlerts.orphans = [];
	syncAlerts.open = false;
	canvas.threads = threads as typeof canvas.threads;
	mockTauriCommand('list_threads', () => threads);
	mockTauriCommand('canvas_load', () => ({ documents: [], threads, relationships: [], milestones: [] }));
	mockTauriCommand('list_previous_values', () => []);
	mockTauriCommand('list_sync_alerts', () => ({ alerts: [], could_not_check: null }));
	waiting = [
		orphanDto('document:a', 'Alpha', 'thread:gone', 'Old drafts'),
		orphanDto('document:b', 'Beta', 'thread:gone', 'Old drafts')
	];
	mockTauriCommand('list_orphaned_documents', () => ({ orphans: waiting, could_not_check: null }));
});

async function openSheet() {
	const r = render(SyncAlerts, { props: { variant: 'mobile' } });
	await reloadSyncAlerts();
	await tick();
	const opener = r.container.querySelector('.notices [aria-haspopup="dialog"]') as HTMLElement | null;
	expect(opener).not.toBeNull();
	await fireEvent.click(opener!);
	await tick();
	const dialog = r.container.querySelector('[role="dialog"]') as HTMLElement;
	expect(dialog).not.toBeNull();
	return { ...r, dialog };
}

describe('SyncAlerts — a group’s action acts on the documents shown (T2)', () => {
	it('“Put all in the trash” sends the group’s documents as drawn, not one that joined later', async () => {
		let sent: { threadId?: string; docIds?: string[]; resolution?: unknown } | null = null;
		mockTauriCommand('resolve_orphans_of_thread', (a: { threadId: string; docIds?: string[]; resolution: unknown }) => {
			sent = a;
			const ids = a.docIds ?? [];
			waiting = waiting.filter((o) => !ids.includes(o.doc_id));
			return ids.length;
		});
		const { dialog } = await openSheet();
		// Gamma arrives by sync for the same deleted thread; the sheet was not redrawn.
		waiting = [...waiting, orphanDto('document:g', 'Gamma', 'thread:gone', 'Old drafts')];
		const group = within(dialog).getByRole('group', { name: /old drafts/i });
		const all = within(group).getByRole('button', { name: (n: string) => /all/i.test(n) && /trash/i.test(n) });
		await fireEvent.click(all);
		await tick();
		await tick();
		expect(sent).not.toBeNull();
		expect(sent!.threadId).toBe('thread:gone');
		expect([...(sent!.docIds ?? [])].sort()).toEqual(['document:a', 'document:b']);
		expect(sent!.resolution).toEqual({ kind: 'trash' });
		// The next refresh lists the late arrival.
		expect(syncAlerts.orphans.map((o) => o.docId)).toEqual(['document:g']);
	});

	it('“Move all to a thread…” sends the group’s documents as drawn', async () => {
		let sent: { docIds?: string[]; resolution?: { kind: string } } | null = null;
		mockTauriCommand('resolve_orphans_of_thread', (a: { docIds?: string[]; resolution: { kind: string } }) => {
			sent = a;
			return (a.docIds ?? []).length;
		});
		const { dialog } = await openSheet();
		waiting = [...waiting, orphanDto('document:g', 'Gamma', 'thread:gone', 'Old drafts')];
		const group = within(dialog).getByRole('group', { name: /old drafts/i });
		await fireEvent.click(within(group).getByRole('button', { name: (n: string) => /all/i.test(n) && /move/i.test(n) }));
		await tick();
		const select = within(dialog).getByRole('combobox', { name: /thread/i }) as HTMLSelectElement;
		await fireEvent.change(select, { target: { value: 'thread:r' } });
		await fireEvent.click(within(dialog).getByRole('button', { name: /^move$/i }));
		await tick();
		await tick();
		expect(sent).not.toBeNull();
		expect([...(sent!.docIds ?? [])].sort()).toEqual(['document:a', 'document:b']);
		expect(sent!.resolution?.kind).toBe('move_to');
	});
});
