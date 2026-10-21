/** S5-12 W4/W5/W6 on the phone — documents whose thread was deleted, in its
 *  alerts view (`doc/plans/C-DOC-adoption.md`, *S5-12*: "Phone: the same
 *  section in its alerts view, if it has one" — it has: `SyncAlerts`, the
 *  mobile sheet). Written before the code by another author.
 *
 *  The DOM contract kept to what a person meets:
 *  - a notice (role="status") saying documents wait, with a button that opens
 *    the alerts sheet (aria-haspopup="dialog"), also when there is no alert;
 *  - in the sheet, a heading "Documents whose thread was deleted"; one list
 *    item per document, naming it and its deleted thread, with three buttons —
 *    "Move to a thread…", "New thread…", and one naming the trash;
 *  - "Move to a thread…" shows a select labelled "Thread" (the live threads)
 *    and a "Move" button; "New thread…" shows a text field labelled "Thread
 *    name" (placeholder "Thread name…", as the new-thread gesture, THREAD-name
 *    N1/N5) and a "Create" button, inactive while the name is empty;
 *  - for each deleted thread, a group (role="group") named after it, holding
 *    the same three actions for all its documents, each named with "all"
 *    (e.g. "Put all in the trash"). */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, fireEvent, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { canvas } from '$lib/stores/canvas.svelte';
import SyncAlerts from './SyncAlerts.svelte';
import { syncAlerts, reloadSyncAlerts } from '$lib/stores/syncAlerts.svelte';

const HEADING = 'Documents whose thread was deleted';
const MARKER = 'zebra-orphan-text-12';

const orphanDto = (id: string, title: string, threadId: string, old: string | null) => ({
	doc_id: id,
	title,
	thread_id: threadId,
	deleted_thread_name: old,
	device: `peer-${id}`,
	device_name: 'Laptop'
});

const alertDto = {
	doc_id: 'document:x',
	title: 'Notes x',
	device: 'peer-x',
	device_name: 'Laptop',
	finding: 'injected instructions in the new content',
	previous_version: null
};

const threads = [
	{ id: 'thread:k', name: 'Keep', description: '', created_at: '2026-01-01T00:00:00Z' },
	{ id: 'thread:r', name: 'Research', description: '', created_at: '2026-01-02T00:00:00Z' }
];

/** What the host lists; resolutions remove from it. */
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
	mockTauriCommand('list_sync_alerts', () => []);
	waiting = [
		orphanDto('document:a', 'Alpha', 'thread:gone', 'Old drafts'),
		orphanDto('document:b', 'Beta', 'thread:gone', 'Old drafts'),
		orphanDto('document:c', 'Gamma', 'thread:other', 'Other')
	];
	mockTauriCommand('list_orphaned_documents', () => waiting);
});

async function load() {
	await reloadSyncAlerts();
	await tick();
}

/** Renders the mobile sheet with the documents waiting, and opens it. */
async function openSheet() {
	const r = render(SyncAlerts, { props: { variant: 'mobile' } });
	await load();
	const opener = r.container.querySelector('.notices [aria-haspopup="dialog"]') as HTMLElement | null;
	expect(opener, 'a notice button opens the sheet').not.toBeNull();
	await fireEvent.click(opener!);
	await tick();
	const dialog = r.container.querySelector('[role="dialog"]') as HTMLElement;
	expect(dialog).not.toBeNull();
	return { ...r, dialog };
}

function itemOf(dialog: HTMLElement, title: string): HTMLElement {
	// The innermost list item naming the document (a group may hold items).
	const items = Array.from(dialog.querySelectorAll('li')) as HTMLElement[];
	const li = items.find(
		(x) =>
			x.textContent?.includes(title) &&
			!Array.from(x.querySelectorAll('li')).some((inner) => inner.textContent?.includes(title))
	);
	expect(li, `a list item for ${title}`).toBeTruthy();
	return li!;
}

describe('SyncAlerts — documents whose thread was deleted (W5)', () => {
	it('says documents wait, in a status notice, even with no alert', async () => {
		const { container } = render(SyncAlerts, { props: { variant: 'mobile' } });
		await load();
		const status = container.querySelector('.notices [role="status"]');
		expect(status?.textContent).toContain('3');
		expect(container.querySelector('[role="alert"]'), 'not an alert: nothing looks suspicious').toBeNull();
	});

	it('shows the section in the sheet, each document with its deleted thread and no text', async () => {
		const { dialog } = await openSheet();
		expect(within(dialog).getByRole('heading', { name: HEADING })).toBeTruthy();
		for (const [title, old] of [
			['Alpha', 'Old drafts'],
			['Beta', 'Old drafts'],
			['Gamma', 'Other']
		]) {
			const li = itemOf(dialog, title);
			expect(li.textContent).toContain(old);
			expect(li.textContent?.toLowerCase()).toContain('deleted');
		}
		expect(dialog.textContent).not.toContain(MARKER);
	});

	it('offers the three ways out on each document', async () => {
		const { dialog } = await openSheet();
		const li = itemOf(dialog, 'Alpha');
		const w = within(li);
		expect(w.getByRole('button', { name: /move to a thread/i })).toBeTruthy();
		expect(w.getByRole('button', { name: /new thread/i })).toBeTruthy();
		expect(w.getByRole('button', { name: /trash/i })).toBeTruthy();
	});

	it('leaves the peer alerts as they are (W6)', async () => {
		mockTauriCommand('list_sync_alerts', () => [alertDto]);
		const { container, dialog } = await openSheet();
		expect(container.querySelector('[role="alert"]')?.textContent).toContain('A change from Laptop to “Notes x” looks suspicious');
		expect(within(dialog).getAllByText('Open the document').length).toBe(1);
		expect(syncAlerts.alerts.map((a) => a.docId)).toEqual(['document:x']);
	});
});

describe('SyncAlerts — the three ways out (W4)', () => {
	it('puts a document in the trash, and drops it from the section', async () => {
		const resolved = vi.fn();
		mockTauriCommand('resolve_orphan', (a: { docId: string; resolution: unknown }) => {
			resolved(a.docId, a.resolution);
			waiting = waiting.filter((o) => o.doc_id !== a.docId);
		});
		const { dialog } = await openSheet();
		await fireEvent.click(within(itemOf(dialog, 'Alpha')).getByRole('button', { name: /trash/i }));
		await tick();
		await tick();
		expect(resolved).toHaveBeenCalledWith('document:a', { kind: 'trash' });
		expect(syncAlerts.orphans.map((o) => o.docId)).not.toContain('document:a');
		expect(dialog.textContent).not.toContain('Alpha');
	});

	it('moves a document to a thread chosen among the live threads', async () => {
		const resolved = vi.fn();
		mockTauriCommand('resolve_orphan', (a: { docId: string; resolution: unknown }) => {
			resolved(a.docId, a.resolution);
			waiting = waiting.filter((o) => o.doc_id !== a.docId);
		});
		const { dialog } = await openSheet();
		await fireEvent.click(within(itemOf(dialog, 'Beta')).getByRole('button', { name: /move to a thread/i }));
		await tick();
		const select = within(dialog).getByLabelText(/^thread$/i) as HTMLSelectElement;
		const offered = Array.from(select.options).map((o) => o.value);
		expect(offered).toEqual(['thread:k', 'thread:r']);
		await fireEvent.change(select, { target: { value: 'thread:r' } });
		await fireEvent.click(within(dialog).getByRole('button', { name: /^move$/i }));
		await tick();
		await tick();
		expect(resolved).toHaveBeenCalledWith('document:b', { kind: 'move_to', thread_id: 'thread:r' });
		expect(syncAlerts.orphans.map((o) => o.docId)).not.toContain('document:b');
	});

	it('creates a new thread named by the user, and refuses an empty name', async () => {
		const resolved = vi.fn();
		mockTauriCommand('resolve_orphan', (a: { docId: string; resolution: { kind: string; name?: string } }) => {
			resolved(a.docId, a.resolution);
			waiting = waiting.filter((o) => o.doc_id !== a.docId);
		});
		const { dialog } = await openSheet();
		await fireEvent.click(within(itemOf(dialog, 'Gamma')).getByRole('button', { name: /new thread/i }));
		await tick();
		const field = within(dialog).getByLabelText(/^thread name$/i) as HTMLInputElement;
		expect(field.getAttribute('placeholder')).toBe('Thread name…');
		const create = within(dialog).getByRole('button', { name: /^create$/i }) as HTMLButtonElement;
		expect(create.disabled, 'inactive while the name is empty').toBe(true);
		await fireEvent.input(field, { target: { value: '   ' } });
		await tick();
		expect(create.disabled, 'inactive for a spaces-only name').toBe(true);
		await fireEvent.input(field, { target: { value: '  Brought back ' } });
		await tick();
		expect(create.disabled).toBe(false);
		await fireEvent.click(create);
		await tick();
		await tick();
		expect(resolved).toHaveBeenCalledTimes(1);
		const [docId, resolution] = resolved.mock.calls[0];
		expect(docId).toBe('document:c');
		expect(resolution.kind).toBe('new_thread');
		expect(resolution.name.trim()).toBe('Brought back');
	});

	it('a refused way out says why and keeps the document', async () => {
		mockTauriCommand('resolve_orphan', () => {
			throw new Error('That thread does not exist (it was deleted, or never existed). Nothing changed.');
		});
		const { dialog } = await openSheet();
		await fireEvent.click(within(itemOf(dialog, 'Alpha')).getByRole('button', { name: /trash/i }));
		await tick();
		await tick();
		const err = dialog.querySelector('[role="alert"]');
		expect(err?.textContent?.trim().length ?? 0).toBeGreaterThan(0);
		expect(syncAlerts.orphans.map((o) => o.docId)).toContain('document:a');
		expect(dialog.textContent).toContain('Alpha');
	});

	it('resolves all documents of a deleted thread together', async () => {
		const grouped = vi.fn();
		mockTauriCommand('resolve_orphans_of_thread', (a: { threadId: string; resolution: unknown }) => {
			grouped(a.threadId, a.resolution);
			waiting = waiting.filter((o) => o.thread_id !== a.threadId);
			return 2;
		});
		const { dialog } = await openSheet();
		const group = within(dialog).getByRole('group', { name: /old drafts/i });
		// The group's own action says it is for all of them.
		const all = within(group).getByRole('button', { name: (n: string) => /all/i.test(n) && /trash/i.test(n) });
		await fireEvent.click(all);
		await tick();
		await tick();
		expect(grouped).toHaveBeenCalledWith('thread:gone', { kind: 'trash' });
		expect(syncAlerts.orphans.map((o) => o.docId)).toEqual(['document:c']);
	});
});
