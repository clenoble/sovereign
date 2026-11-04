/** v0.0.12 item 7 V2b, B4–B5 at the store — the documents the conversion
 *  could not read (`doc/plans/V0012-item7-v2b-app-list.md`). Written before
 *  the code, by another author: whoever implements V2b makes these pass
 *  without editing this file.
 *
 *  Seams named here (the code author creates them; the component's side is
 *  `SyncAlerts.unreadable.svelte.test.ts`, the Rust side
 *  `crates/sovereign-app/src/item7_v2b_tests.rs`):
 *  - `$lib/api/commands`: `listUnreadableDocuments(): Promise<UnreadableDocumentsDto>`
 *    invoking `list_unreadable_documents` (no arguments), and
 *    `UnreadableDocumentsDto { heading: string; meaning: string; lines: string[] }`;
 *  - `$lib/stores/syncAlerts.svelte`: `syncAlerts.unreadable: { heading;
 *    meaning; lines }`, loaded by `reloadSyncAlerts`; `unreadableNotice():
 *    string | null`;
 *  - `$lib/api/events`: on `documents-migrated`, the list is reloaded (a
 *    retry may have emptied it), and the migration notice still shows.
 *
 *  **What "cleared when the store changes (lock, persona switch)" can reach
 *  here.** The frontend has no lock or persona-switch signal: `app.authState`
 *  goes to `'ready'` at login and nothing sets it back, and the backend emits
 *  no lock event (searched 2026-10-25: `authState` assignments in
 *  `LoginScreen.svelte`, `OnboardingWizard.svelte`, `+layout.svelte`; the
 *  `emit(` calls in `crates/sovereign-app/src`). A persona switch restarts
 *  the frontend's state with the app. What is pinned is what reaches the
 *  store: each reload replaces the list with the backend's answer for the
 *  store open now, and a refused command (the session locked) empties it —
 *  never keeping a list read before. No signal is invented here.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import { mockTauriCommand } from '$lib/test/tauri';
import { listUnreadableDocuments } from '$lib/api/commands';
import { syncAlerts, reloadSyncAlerts, unreadableNotice } from '$lib/stores/syncAlerts.svelte';
import { migrationNotice } from '$lib/stores/migrationNotice.svelte';

const HEADING = 'Documents that could not be read';
const MEANING = 'Each one is kept as it was, untouched. Sovereign will try to read it again after its next update.';
const LINE_A = 'Document k7f3a9c2 — last modified 2025-03-02 12:34';
const LINE_B = 'Document 0b1e44aa (in the trash) — last modified 2025-02-11 08:05';

let lines: string[] = [];
let calls = 0;

beforeEach(async () => {
	syncAlerts.alerts = [];
	syncAlerts.previousValues = [];
	syncAlerts.olderDevices = [];
	syncAlerts.orphans = [];
	syncAlerts.open = false;
	mockTauriCommand('list_sync_alerts', () => []);
	mockTauriCommand('list_previous_values', () => []);
	mockTauriCommand('list_orphaned_documents', () => []);
	lines = [];
	mockTauriCommand('list_unreadable_documents', () => {
		calls++;
		return { heading: HEADING, meaning: MEANING, lines };
	});
	await reloadSyncAlerts();
	calls = 0;
});

describe('the command wrapper', () => {
	it('invokes list_unreadable_documents and gives its answer as is', async () => {
		lines = [LINE_A];
		const got = await listUnreadableDocuments();
		expect(got).toEqual({ heading: HEADING, meaning: MEANING, lines: [LINE_A] });
		expect(calls).toBe(1);
	});
});

describe('the store (B5: loaded with the other alerts)', () => {
	it('starts empty', () => {
		expect(syncAlerts.unreadable.lines).toEqual([]);
		expect(unreadableNotice()).toBeNull();
	});

	it('reloadSyncAlerts loads the heading, the meaning and the lines, in order', async () => {
		lines = [LINE_A, LINE_B];
		await reloadSyncAlerts();
		expect(calls).toBe(1);
		expect(syncAlerts.unreadable).toEqual({ heading: HEADING, meaning: MEANING, lines: [LINE_A, LINE_B] });
	});

	it('a reload replaces the list with the answer (emptied when none remain)', async () => {
		lines = [LINE_A, LINE_B];
		await reloadSyncAlerts();
		lines = [LINE_B];
		await reloadSyncAlerts();
		expect(syncAlerts.unreadable.lines).toEqual([LINE_B]);
		lines = [];
		await reloadSyncAlerts();
		expect(syncAlerts.unreadable.lines).toEqual([]);
		expect(unreadableNotice()).toBeNull();
	});

	it('a refused command (locked) empties the list instead of keeping the last one', async () => {
		lines = [LINE_A];
		await reloadSyncAlerts();
		mockTauriCommand('list_unreadable_documents', () => {
			throw 'The workspace is locked: log in to change documents.';
		});
		await expect(reloadSyncAlerts()).resolves.toBeUndefined();
		expect(syncAlerts.unreadable.lines).toEqual([]);
	});

	it('the other lists still load when this command fails (a host without it)', async () => {
		mockTauriCommand('list_unreadable_documents', () => {
			throw new Error('unknown command');
		});
		mockTauriCommand('list_orphaned_documents', () => [
			{ doc_id: 'document:a', title: 'Alpha', thread_id: 'thread:g', deleted_thread_name: null, device: 'p', device_name: '' }
		]);
		await reloadSyncAlerts();
		expect(syncAlerts.orphans.map((o) => o.docId)).toEqual(['document:a']);
	});

	it('the dialog is not closed by a reload while only this list is non-empty', async () => {
		lines = [LINE_A];
		await reloadSyncAlerts();
		syncAlerts.open = true;
		await reloadSyncAlerts();
		expect(syncAlerts.open).toBe(true);
	});
});

describe('the notice (B4)', () => {
	it('"1 document could not be read"', async () => {
		lines = [LINE_A];
		await reloadSyncAlerts();
		expect(unreadableNotice()).toBe('1 document could not be read');
	});

	it('"N documents could not be read"', async () => {
		lines = [LINE_A, LINE_B];
		await reloadSyncAlerts();
		expect(unreadableNotice()).toBe('2 documents could not be read');
	});
});

describe('documents-migrated (B5: a retry may have emptied the list)', () => {
	it('reloads the list, and the migration notice still shows', async () => {
		const handlers = new Map<string, (e: { payload: unknown }) => unknown>();
		vi.mocked(listen).mockImplementation((async (name: string, h: (e: { payload: unknown }) => unknown) => {
			handlers.set(name, h);
			return () => {};
		}) as unknown as typeof listen);
		const { subscribeToEvents } = await import('$lib/api/events');
		const unlisten = await subscribeToEvents();
		try {
			lines = [LINE_A, LINE_B];
			await reloadSyncAlerts();
			expect(syncAlerts.unreadable.lines).toHaveLength(2);
			calls = 0;

			lines = [];
			const handler = handlers.get('documents-migrated');
			expect(handler, 'a listener on documents-migrated').toBeTruthy();
			await handler!({ payload: { text: '2 document(s) converted to the new format; 0 could not be read' } });
			await vi.waitFor(() => expect(calls).toBeGreaterThan(0));
			await vi.waitFor(() => expect(syncAlerts.unreadable.lines).toEqual([]));
			expect(migrationNotice.text).toContain('converted');
		} finally {
			unlisten();
			vi.mocked(listen).mockReset();
			vi.mocked(listen).mockImplementation((async () => () => {}) as unknown as typeof listen);
		}
	});
});
