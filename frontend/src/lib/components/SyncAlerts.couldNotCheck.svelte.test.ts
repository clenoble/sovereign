/** The Alerts' lists that cannot be read, in the app's Alerts dialog and its
 *  notices, desktop and mobile (spec ch. 03, *What cannot be read is never
 *  shown as empty*). Written before the code, by another author: whoever
 *  implements it makes these pass without editing this file; a test that
 *  looks wrong is reported, not worked around. The Rust side is tested in
 *  `crates/sovereign-app/src/could_not_check_tests.rs`.
 *
 *  What was decided (2026-11-17, the Fable maille by delegation of Céline; the
 *  scope by PANDA2, the contract's author: the three lists of the Alerts, on
 *  both hosts):
 *  - a list that cannot be read is said in its section's place, where its
 *    rows would be, never shown as an empty list;
 *  - a failure counts as something to show: the sync-alerts notice says the
 *    sentence, as it says "1 document could not be read", and the dialog does
 *    not close for want of something to show;
 *  - under the unreadable documents' heading, the meaning line does not show
 *    with the sentence;
 *  - a refused call (a locked session) is not a "could not check": it still
 *    shows nothing, and a sentence read under another session never stays on
 *    screen (the rule of v0.0.12 item 7 V2b, B5).
 *
 *  The seam (`$lib/api/commands`, the test author's « types and signatures »
 *  commit): `list_sync_alerts` → `SyncAlertsDto { alerts, could_not_check }`;
 *  `list_orphaned_documents` → `OrphansDto { orphans, could_not_check }`;
 *  `list_unreadable_documents` → `UnreadableDocumentsDto` with
 *  `could_not_check`, present only when set. The sentence comes worded by the backend
 *  (`sovereign-docs`): the frontend holds no copy (the last test).
 *
 *  Read here: what a person meets (the dialog's text and order, the live
 *  regions of the notices), not the store's fields, which are the
 *  implementer's.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { render, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { mockTauriCommand } from '$lib/test/tauri';
import { canvas } from '$lib/stores/canvas.svelte';
import SyncAlerts from './SyncAlerts.svelte';
import { syncAlerts, reloadSyncAlerts } from '$lib/stores/syncAlerts.svelte';

/** `sovereign_docs::COULD_NOT_CHECK_ALERTS`, `_ORPHANS`, `_UNREADABLE`, as the
 *  backend sends them (decided 2026-11-17). */
const ALERTS = 'Could not check for suspicious changes from your devices.';
const ORPHANS = 'Could not check for documents whose thread was deleted.';
const UNREADABLE = 'Could not check for unreadable documents.';

/** `sovereign_docs::UNREADABLE_HEADING` and `UNREADABLE_MEANING`, which the
 *  backend still sends with the sentence. */
const HEADING = 'Documents that could not be read';
const MEANING = 'Each one is kept as it was, untouched. Sovereign will try to read it again after its next update.';

/** What the dialog says when there are no alerts: never in place of alerts
 *  that could not be read. */
const NO_ALERTS = 'No alerts.';

const LOCKED = 'The workspace is locked: log in to change documents.';

/** What each command answers: the sentence when its list could not be read,
 *  `null` when it read; `refused` makes it throw (a locked session). */
let alertsSaid: string | null = null;
let orphansSaid: string | null = null;
let unreadableSaid: string | null = null;
let refused: Set<string> = new Set();

function answer<T>(cmd: string, value: () => T): () => T {
	return () => {
		if (refused.has(cmd)) throw LOCKED;
		return value();
	};
}

beforeEach(async () => {
	syncAlerts.alerts = [];
	syncAlerts.previousValues = [];
	syncAlerts.olderDevices = [];
	syncAlerts.newerDevices = [];
	syncAlerts.orphans = [];
	syncAlerts.open = false;
	canvas.threads = [];
	alertsSaid = null;
	orphansSaid = null;
	unreadableSaid = null;
	refused = new Set();
	mockTauriCommand('list_threads', () => []);
	mockTauriCommand('canvas_load', () => ({ documents: [], threads: [], relationships: [], milestones: [] }));
	mockTauriCommand('list_previous_values', () => []);
	mockTauriCommand(
		'list_sync_alerts',
		answer('list_sync_alerts', () => ({ alerts: [], could_not_check: alertsSaid }))
	);
	mockTauriCommand(
		'list_orphaned_documents',
		answer('list_orphaned_documents', () => ({ orphans: [], could_not_check: orphansSaid }))
	);
	mockTauriCommand(
		'list_unreadable_documents',
		answer('list_unreadable_documents', () => ({
			heading: HEADING,
			meaning: MEANING,
			lines: [],
			// Sent only when set, as the backend does (`skip_serializing_if`).
			...(unreadableSaid ? { could_not_check: unreadableSaid } : {})
		}))
	);
	// Start from lists that read and are empty (whatever the previous test left).
	await reloadSyncAlerts();
	syncAlerts.open = false;
});

async function load() {
	await reloadSyncAlerts();
	await tick();
}

function dialogOf(container: HTMLElement): HTMLElement | null {
	return container.querySelector('[role="dialog"]');
}

/** The live regions of the notices (role="status" or role="alert"), outside
 *  the dialog. */
function notices(container: HTMLElement): HTMLElement[] {
	const all = Array.from(container.querySelectorAll('[role="status"], [role="alert"]')) as HTMLElement[];
	const dialog = dialogOf(container);
	return all.filter((n) => !dialog || !dialog.contains(n));
}

/** The innermost element of `root` whose own text holds `text`. */
function elementWith(root: HTMLElement, text: string): HTMLElement | null {
	const all = Array.from(root.querySelectorAll('*')) as HTMLElement[];
	const holding = all.filter((e) => (e.textContent ?? '').includes(text));
	return holding.find((e) => !holding.some((o) => o !== e && e.contains(o))) ?? null;
}

/** Whether `a` comes before `b` in the document. */
function before(a: Node, b: Node): boolean {
	return (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
}

/** Sets the list `which` as not read: its command answers the sentence. */
function cannotBeRead(which: 'alerts' | 'orphans' | 'unreadable') {
	if (which === 'alerts') alertsSaid = ALERTS;
	if (which === 'orphans') orphansSaid = ORPHANS;
	if (which === 'unreadable') unreadableSaid = UNREADABLE;
}

const COMMAND = {
	alerts: 'list_sync_alerts',
	orphans: 'list_orphaned_documents',
	unreadable: 'list_unreadable_documents'
} as const;

const SENTENCE = { alerts: ALERTS, orphans: ORPHANS, unreadable: UNREADABLE } as const;

const CASES = ['alerts', 'orphans', 'unreadable'] as const;

describe.each(['desktop', 'mobile'] as const)('SyncAlerts (%s) — a list that could not be read', (variant) => {
	// ------------------------------------------------- in the section's place --

	it('the alerts: the sentence where the alerts would be, never "No alerts."', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		syncAlerts.open = true;
		await tick();
		cannotBeRead('alerts');
		await load();
		const dialog = dialogOf(container);
		expect(dialog, 'the dialog stays open: a failure counts as something to show').not.toBeNull();
		const said = elementWith(dialog!, ALERTS);
		expect(said, 'the sentence, in the dialog').not.toBeNull();
		expect(dialog!.textContent, 'never shown as no alerts').not.toContain(NO_ALERTS);
		const title = within(dialog!).getByRole('heading', { level: 2, name: 'Alerts' });
		expect(before(title, said!), 'under the dialog’s title, where the alerts would be').toBe(true);
		const firstSection = dialog!.querySelector('h3');
		if (firstSection) expect(before(said!, firstSection), 'before the other sections').toBe(true);
	});

	it('the documents whose thread was deleted: the sentence after the alerts, in their section’s place', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		syncAlerts.open = true;
		await tick();
		cannotBeRead('orphans');
		await load();
		const dialog = dialogOf(container);
		expect(dialog, 'the dialog stays open: a failure counts as something to show').not.toBeNull();
		const said = elementWith(dialog!, ORPHANS);
		expect(said, 'the sentence, in the dialog').not.toBeNull();
		const noAlerts = elementWith(dialog!, NO_ALERTS);
		expect(noAlerts, 'the alerts read, and there are none').not.toBeNull();
		expect(before(noAlerts!, said!), 'after the alerts').toBe(true);
	});

	it('the documents that could not be read: the sentence under their heading, without the meaning', async () => {
		const { container } = render(SyncAlerts, { props: { variant } });
		syncAlerts.open = true;
		await tick();
		cannotBeRead('unreadable');
		await load();
		const dialog = dialogOf(container);
		expect(dialog, 'the dialog stays open: a failure counts as something to show').not.toBeNull();
		const heading = within(dialog!).getByRole('heading', { level: 3, name: HEADING });
		const said = elementWith(dialog!, UNREADABLE);
		expect(said, 'the sentence, in the dialog').not.toBeNull();
		expect(before(heading, said!), 'under the heading').toBe(true);
		expect(dialog!.textContent, 'the meaning line does not show with it').not.toContain(MEANING);
		expect(within(dialog!).queryAllByRole('listitem'), 'no line is made up').toEqual([]);
	});

	// ------------------------------------------------------------ the notice --

	it.each(CASES)('%s: the notice says the sentence, with nothing else waiting', async (which) => {
		const { container } = render(SyncAlerts, { props: { variant } });
		cannotBeRead(which);
		await load();
		const said = notices(container).filter((n) => (n.textContent ?? '').includes(SENTENCE[which]));
		expect(said.length, `a live region says “${SENTENCE[which]}”`).toBe(1);
		// A failure is said with role="alert", as the app says every failure; status is for what
		// waits (the Fable maille, by delegation of Céline, 2026-11-17).
		expect(said[0].getAttribute('role'), 'a failure is an alert').toBe('alert');
		for (const other of CASES.filter((c) => c !== which)) {
			expect(container.textContent, `only what could not be read is said`).not.toContain(SENTENCE[other]);
		}
	});

	// ----------------------------------------------- a refusal is not a failure --

	it.each(CASES)('%s: a refused call (a locked session) still shows nothing', async (which) => {
		const { container } = render(SyncAlerts, { props: { variant } });
		refused.add(COMMAND[which]);
		await load();
		expect(notices(container), 'no notice').toEqual([]);
		expect(container.textContent).not.toContain('Could not check');
		syncAlerts.open = true;
		await tick();
		expect(dialogOf(container)!.textContent, 'nor in the dialog').not.toContain('Could not check');
	});

	it.each(CASES)('%s: a sentence read before is not kept when the call is then refused', async (which) => {
		const { container } = render(SyncAlerts, { props: { variant } });
		cannotBeRead(which);
		await load();
		expect(container.textContent, 'fixture: said first').toContain(SENTENCE[which]);
		refused.add(COMMAND[which]);
		await load();
		expect(container.textContent, 'what was read under another session never stays on screen').not.toContain(
			SENTENCE[which]
		);
	});

	it.each(CASES)('%s: once the list reads again, the sentence goes', async (which) => {
		const { container } = render(SyncAlerts, { props: { variant } });
		cannotBeRead(which);
		await load();
		expect(container.textContent, 'fixture: said first').toContain(SENTENCE[which]);
		alertsSaid = orphansSaid = unreadableSaid = null;
		await load();
		expect(container.textContent).not.toContain(SENTENCE[which]);
		expect(notices(container), 'nothing left to say').toEqual([]);
	});
});

// ------------------------------------------------------- the words, once --

/** The sentences are worded once, in `sovereign-docs`: the frontend holds no
 *  copy of them (test files are left out). */
describe('the sentences on the frontend', () => {
	it('no source file holds a copy of them', () => {
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
			for (const copy of [ALERTS, ORPHANS, UNREADABLE, 'Could not check for']) {
				expect(text.includes(copy), `${f} holds a copy of “${copy}”`).toBe(false);
			}
		}
	});
});
