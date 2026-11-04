/** v0.0.12 item 7 V2b, B3–B6 — the documents the conversion could not read,
 *  in the Alerts dialog, desktop and mobile (`doc/plans/V0012-item7-v2b-app-list.md`).
 *  Written before the code, by another author: whoever implements V2b makes
 *  these pass without editing this file.
 *
 *  Seams named here (the code author creates them):
 *  - the Tauri command `list_unreadable_documents` → `UnreadableDocumentsDto
 *    { heading: string; meaning: string; lines: string[] }` (Rust side:
 *    `crates/sovereign-app/src/item7_v2b_tests.rs`). The heading and the
 *    meaning come from the backend (`sovereign_docs::UNREADABLE_HEADING`,
 *    `UNREADABLE_MEANING`), the lines already worded (B1): the frontend holds
 *    no copy of them;
 *  - `$lib/api/commands`: `listUnreadableDocuments()` and the type
 *    `UnreadableDocumentsDto`;
 *  - `$lib/stores/syncAlerts.svelte`: `syncAlerts.unreadable: { heading:
 *    string; meaning: string; lines: string[] }` (empty: `''`, `''`, `[]`),
 *    loaded by `reloadSyncAlerts` (B5), and `unreadableNotice(): string |
 *    null` (B4). Store-level tests: `syncAlerts.unreadable.svelte.test.ts`.
 *
 *  The DOM contract kept to what a person meets:
 *  - B4: while any remain, a notice (role="status") "1 document could not be
 *    read" / "N documents could not be read", with a button "See it" / "See
 *    them" (aria-haspopup="dialog") that opens the dialog, also with no alert;
 *    its accessible name is F1's (`doc/plans/V0012-frontend-a11y.md`, which
 *    supersedes the name here): "See it: the document that could not be
 *    read" / "See them: the documents that could not be read";
 *  - B3: in the dialog, below the orphans, an h3 with the heading, the
 *    meaning, and the lines as a list labelled by the heading; no button
 *    from the heading to the list's end;
 *  - B6: the notice is announced (role="status", not an alert); the dialog
 *    takes focus when the notice opens it and holds the list (reached by
 *    keyboard: it scrolls); **[choice of the test author]** contrast "as the
 *    dialog's other sections" is read as the same classes: the heading
 *    `section-title`, the meaning `hint`.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { mockTauriCommand } from '$lib/test/tauri';
import { canvas } from '$lib/stores/canvas.svelte';
import SyncAlerts from './SyncAlerts.svelte';
import { syncAlerts, reloadSyncAlerts } from '$lib/stores/syncAlerts.svelte';

/** `sovereign_docs::UNREADABLE_HEADING` and `UNREADABLE_MEANING` (B1). */
const HEADING = 'Documents that could not be read';
const MEANING = 'Each one is kept as it was, untouched. Sovereign will try to read it again after its next update.';

const LINE_A = 'Document k7f3a9c2 — last modified 2025-03-02 12:34';
const LINE_B = 'Document 0b1e44aa (in the trash) — last modified 2025-02-11 08:05';
const LINE_C = 'Document 5d9c0f71 — last modified 2024-12-30 23:59';

const dto = (lines: string[]) => ({ heading: HEADING, meaning: MEANING, lines });

/** What the host lists. */
let lines: string[] = [];

const orphanDto = {
	doc_id: 'document:a',
	title: 'Alpha',
	thread_id: 'thread:gone',
	deleted_thread_name: 'Old drafts',
	device: 'peer-a',
	device_name: 'Laptop'
};

beforeEach(async () => {
	syncAlerts.alerts = [];
	syncAlerts.previousValues = [];
	syncAlerts.olderDevices = [];
	syncAlerts.orphans = [];
	syncAlerts.open = false;
	canvas.threads = [];
	mockTauriCommand('list_threads', () => []);
	mockTauriCommand('canvas_load', () => ({ documents: [], threads: [], relationships: [], milestones: [] }));
	mockTauriCommand('list_sync_alerts', () => []);
	mockTauriCommand('list_previous_values', () => []);
	mockTauriCommand('list_orphaned_documents', () => []);
	lines = [];
	mockTauriCommand('list_unreadable_documents', () => dto(lines));
	// Start from an empty list (whatever the store's previous test left).
	await reloadSyncAlerts();
	lines = [LINE_A, LINE_B, LINE_C];
});

async function load() {
	await reloadSyncAlerts();
	await tick();
}

/** The notice about the documents that could not be read, or null. */
function unreadableNotice(container: HTMLElement): HTMLElement | null {
	const all = Array.from(container.querySelectorAll('[role="status"]')) as HTMLElement[];
	return all.find((x) => /could not be read/.test(x.textContent ?? '')) ?? null;
}

function dialogOf(container: HTMLElement): HTMLElement | null {
	return container.querySelector('[role="dialog"]');
}

/** Opens the dialog from the notice's button. */
async function openFromNotice(container: HTMLElement): Promise<{ dialog: HTMLElement; button: HTMLElement }> {
	const notice = unreadableNotice(container);
	expect(notice, 'the notice').not.toBeNull();
	const button = within(notice!).getByRole('button');
	await fireEvent.click(button);
	await tick();
	const dialog = dialogOf(container);
	expect(dialog, 'the notice opens the dialog').not.toBeNull();
	return { dialog: dialog!, button };
}

/** Whether `a` comes before `b` in the document. */
function before(a: Node, b: Node): boolean {
	return (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
}

describe.each(['desktop', 'mobile'] as const)('SyncAlerts (%s) — documents that could not be read', (variant) => {
	// ------------------------------------------------------------------ B4 --

	it('B4: says how many could not be read, in a status notice, with no alert', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		const notice = unreadableNotice(container);
		expect(notice, 'a role="status" notice').not.toBeNull();
		expect(notice!.textContent).toContain('3 documents could not be read');
		expect(container.querySelector('[role="alert"]'), 'not an alert: nothing looks suspicious').toBeNull();
	});

	it('B4: one document — "1 document could not be read", and "See it"', async () => {
		lines = [LINE_A];
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		const notice = unreadableNotice(container)!;
		expect(notice.textContent).toContain('1 document could not be read');
		expect(notice.textContent).not.toContain('1 documents');
		const button = within(notice).getByRole('button', { name: 'See it: the document that could not be read' });
		expect(button.getAttribute('aria-haspopup')).toBe('dialog');
	});

	it('B4: several — "See them", which opens the dialog', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		const notice = unreadableNotice(container)!;
		const button = within(notice).getByRole('button', { name: 'See them: the documents that could not be read' });
		expect(button.getAttribute('aria-haspopup')).toBe('dialog');
		expect(dialogOf(container)).toBeNull();
		await fireEvent.click(button);
		await tick();
		expect(dialogOf(container), 'the dialog is open').not.toBeNull();
	});

	it('B4: no notice when none remain', async () => {
		lines = [];
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		expect(unreadableNotice(container)).toBeNull();
		expect(container.textContent).not.toContain('could not be read');
	});

	// ------------------------------------------------------------------ B3 --

	it('B3: the heading (h3), the meaning, and the lines as a list labelled by the heading', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		const { dialog } = await openFromNotice(container);
		const w = within(dialog);
		const heading = w.getByRole('heading', { level: 3, name: HEADING });
		const meaning = w.getByText(MEANING);
		const list = w.getByRole('list', { name: HEADING });
		const items = within(list).getAllByRole('listitem');
		expect(items.map((li) => li.textContent?.trim())).toEqual([LINE_A, LINE_B, LINE_C]);
		expect(before(heading, meaning) && before(meaning, list), 'heading, meaning, then the list').toBe(true);
	});

	it('B3: nothing to act on — no button from the heading to the end of the list', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		const { dialog } = await openFromNotice(container);
		const heading = within(dialog).getByRole('heading', { level: 3, name: HEADING });
		const list = within(dialog).getByRole('list', { name: HEADING });
		expect(within(list).queryAllByRole('button')).toEqual([]);
		const inSection = (Array.from(dialog.querySelectorAll('button')) as HTMLElement[]).filter(
			(b) => before(heading, b) && (list.contains(b) || before(b, list) || b === list)
		);
		expect(inSection, 'no button in the section').toEqual([]);
	});

	it('B3: below the orphans', async () => {
		mockTauriCommand('list_orphaned_documents', () => [orphanDto]);
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		const { dialog } = await openFromNotice(container);
		const orphans = within(dialog).getByRole('heading', { name: 'Documents whose thread was deleted' });
		const heading = within(dialog).getByRole('heading', { level: 3, name: HEADING });
		expect(before(orphans, heading), 'the orphans first, then this list').toBe(true);
	});

	it('B3: the dialog shows no such section when none remain', async () => {
		lines = [];
		mockTauriCommand('list_orphaned_documents', () => [orphanDto]);
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		syncAlerts.open = true;
		await tick();
		const dialog = dialogOf(container)!;
		expect(within(dialog).queryByRole('heading', { name: HEADING })).toBeNull();
		expect(dialog.textContent).not.toContain(MEANING);
	});

	// ------------------------------------------------------------------ B5 --

	it('B5: stays while any remain — a reload with the same list keeps it, and the dialog open', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		await openFromNotice(container);
		await load();
		const dialog = dialogOf(container);
		expect(dialog, 'the dialog stays open: the list is not empty').not.toBeNull();
		expect(within(dialog!).getByRole('list', { name: HEADING })).toBeTruthy();
		expect(unreadableNotice(container)?.textContent).toContain('3 documents could not be read');
	});

	it('B5: a reload that finds none empties the list and takes the notice away', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		expect(unreadableNotice(container)).not.toBeNull();
		lines = [];
		await load();
		expect(unreadableNotice(container)).toBeNull();
		expect(syncAlerts.unreadable.lines).toEqual([]);
	});

	it('B5: the command refused (the store locked) — the list is emptied, never kept from before', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		expect(unreadableNotice(container)).not.toBeNull();
		mockTauriCommand('list_unreadable_documents', () => {
			throw 'The workspace is locked: log in to change documents.';
		});
		await load();
		expect(unreadableNotice(container), 'one store’s list never shows under another').toBeNull();
		expect(syncAlerts.unreadable.lines).toEqual([]);
	});

	// ------------------------------------------------------------------ B6 --

	it('B6: the dialog takes focus and holds the list; Escape closes it and gives focus back', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		const { dialog, button } = await openFromNotice(container);
		expect(document.activeElement, 'the dialog takes focus').toBe(dialog);
		expect(dialog.contains(within(dialog).getByRole('list', { name: HEADING }))).toBe(true);
		await fireEvent.keyDown(dialog, { key: 'Escape' });
		await tick();
		expect(dialogOf(container)).toBeNull();
		expect(document.activeElement, 'focus back on the notice’s button').toBe(button);
	});

	it('B6: the same classes as the dialog’s other sections (contrast)', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		await load();
		const { dialog } = await openFromNotice(container);
		const heading = within(dialog).getByRole('heading', { level: 3, name: HEADING });
		expect(heading.classList.contains('section-title')).toBe(true);
		expect(within(dialog).getByText(MEANING).classList.contains('hint')).toBe(true);
	});
});

// -------------------------------------------------------------- B1, frontend --

/** B1: the frontend holds no copy of the heading or the meaning; they come
 *  from the backend with the lines. (Test files are left out.) */
describe('B1 on the frontend', () => {
	it('no source file holds the heading or the meaning', () => {
		const files: string[] = [];
		const walk = (dir: string) => {
			for (const name of readdirSync(dir)) {
				const p = join(dir, name);
				if (statSync(p).isDirectory()) walk(p);
				else if (/\.(ts|svelte|js)$/.test(name) && !/\.test\.ts$/.test(name)) files.push(p);
			}
		};
		walk('src');
		expect(files.some((f) => f.endsWith('SyncAlerts.svelte')), 'fixture: the scan sees SyncAlerts.svelte').toBe(true);
		for (const f of files) {
			const text = readFileSync(f, 'utf8');
			for (const copy of [HEADING, 'kept as it was, untouched']) {
				expect(text.includes(copy), `${f} holds a copy of “${copy}”`).toBe(false);
			}
		}
	});
});
