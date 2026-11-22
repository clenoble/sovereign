/** The phone's History that cannot be read (item C5 of v0.0.13; spec ch. 03,
 *  *What cannot be read is never shown as empty*). Written before the code, by
 *  another author: whoever implements it makes these pass without editing this
 *  file; a test that looks wrong is reported, not worked around. The Rust side
 *  is tested in `crates/sovereign-app/src/c5_history_tests.rs`.
 *
 *  What was decided (2026-11-18, the Fable maille by delegation of Céline):
 *  - versions that cannot be read are said in their place, never as « No saved
 *    versions yet »; a list that read and is empty still says it;
 *  - one version that cannot be read stays in the list; its preview is the
 *    backend's sentence, and « Use this version » is not offered for it;
 *  - the documents whose history this session could not read are counted, and
 *    the phone's sync alerts say the count;
 *  - a refused call (a locked session) is not « could not be read ».
 *
 *  The seam (`$lib/api/commands`, the test author's « types and signatures »
 *  commit): `list_commits` → `CommitListDto { commits, could_not_read }`, each
 *  `CommitSummary` with its own `could_not_read`; `histories_not_read` →
 *  `HistoriesNotReadDto { count, sentence }`. The sentences come worded by the
 *  backend (`sovereign-docs`): the frontend holds no copy (the last test).
 *
 *  Read here: what a person meets (the reader's text, its buttons, the
 *  notices' live regions), not the stores' fields, which are the implementer's.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, render, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { mockTauriCommand } from '$lib/test/tauri';
import { canvas } from '$lib/stores/canvas.svelte';
import { openById, panels } from '$lib/stores/documents.svelte';
import { syncAlerts, reloadSyncAlerts } from '$lib/stores/syncAlerts.svelte';
import MobileDocReader from './MobileDocReader.svelte';
import SyncAlerts from '../SyncAlerts.svelte';

/** `sovereign_docs::COULD_NOT_READ_HISTORY`, `VERSION_COULD_NOT_BE_READ` and
 *  `histories_not_read_sentence(1)`, as the backend sends them (decided
 *  2026-11-18). */
const HISTORY = "This document's history could not be read.";
const VERSION = 'This version could not be read.';
const ONE_HISTORY = 'The history of 1 document could not be read.';

/** What the phone says when a document has no versions. */
const NO_VERSIONS = 'No saved versions yet';
const USE = 'Use this version';
const LOCKED = 'The workspace is locked: log in to change documents.';

let seq = 0;

function doc(id: string) {
	return {
		id,
		title: 'Notes',
		body: 'some text',
		images: [],
		videos: [],
		thread_id: 'thread:t',
		is_owned: true,
		created_at: '2026-11-18T08:00:00Z',
		modified_at: '2026-11-18T09:00:00Z',
		revision: 'r1'
	};
}

function version(id: string, timestamp: string, readable: boolean) {
	return {
		id,
		message: 'Saved',
		timestamp,
		snapshot_title: readable ? 'Notes' : '',
		snapshot_preview: readable ? 'some text' : '',
		could_not_read: readable ? null : VERSION
	};
}

/** Opens a fresh document in the reader with `list_commits` answering
 *  `answer`, and turns to its History. */
async function historyOf(answer: () => unknown) {
	const id = `document:c5-${++seq}`;
	mockTauriCommand('get_document', () => doc(id));
	mockTauriCommand('get_document_labelled', () => ({ title: 'Notes', body: 'some text' }));
	mockTauriCommand('list_commits', answer);
	await openById(id);
	const panel = panels.find((p) => p.doc.id === id);
	expect(panel, 'fixture: the document opens').toBeTruthy();
	const r = render(MobileDocReader, { props: { panel: panel! } });
	await fireEvent.click(r.getByRole('button', { name: 'History' }));
	await settle();
	return r;
}

async function settle() {
	for (let i = 0; i < 5; i++) {
		await tick();
		await new Promise((resolve) => setTimeout(resolve, 0));
	}
}

beforeEach(() => {
	panels.splice(0, panels.length);
});

// ------------------------------------------------------- the whole list --

describe('the phone’s History — versions that could not be read', () => {
	it('says the sentence in their place, never "No saved versions yet"', async () => {
		const { container } = await historyOf(() => ({ commits: [], could_not_read: HISTORY }));
		const text = container.textContent ?? '';
		expect(text, 'the backend’s sentence').toContain(HISTORY);
		expect(text, 'never shown as no versions').not.toContain(NO_VERSIONS);
	});

	it('a list that read and is empty still says "No saved versions yet"', async () => {
		const { container } = await historyOf(() => ({ commits: [], could_not_read: null }));
		const text = container.textContent ?? '';
		expect(text).toContain(NO_VERSIONS);
		expect(text).not.toContain(HISTORY);
	});

	it('a refused call (a locked session) is not "could not be read"', async () => {
		const { container } = await historyOf(() => {
			throw LOCKED;
		});
		expect(container.textContent ?? '').not.toContain(HISTORY);
	});
});

// ------------------------------------------------------- one version --

describe('the phone’s History — one version that could not be read', () => {
	const answer = () => ({
		commits: [
			version('commit:newer', '2026-11-18T09:30:00Z', false),
			version('commit:older', '2026-11-18T09:00:00Z', true)
		],
		could_not_read: null
	});

	it('stays in the list, beside the one that reads', async () => {
		const { getByRole } = await historyOf(answer);
		const list = getByRole('list', { name: 'Versions' });
		expect(within(list).getAllByRole('button').length, 'both versions are listed').toBe(2);
	});

	it('its preview is the backend’s sentence, and "Use this version" is not offered', async () => {
		const { getByRole, container } = await historyOf(answer);
		const rows = within(getByRole('list', { name: 'Versions' })).getAllByRole('button');
		await fireEvent.click(rows[0]);
		await settle();
		const text = container.textContent ?? '';
		expect(text, 'the version is said').toContain(VERSION);
		expect(text, 'never as no versions').not.toContain(NO_VERSIONS);
		expect(Array.from(container.querySelectorAll('button')).some((b) => (b.textContent ?? '').includes(USE)), 'no "Use this version" for it').toBe(false);
	});

	it('the version that reads keeps its preview and its "Use this version"', async () => {
		const { getByRole, container } = await historyOf(answer);
		const rows = within(getByRole('list', { name: 'Versions' })).getAllByRole('button');
		await fireEvent.click(rows[1]);
		await settle();
		const text = container.textContent ?? '';
		expect(text).toContain('some text');
		expect(text).not.toContain(VERSION);
		expect(Array.from(container.querySelectorAll('button')).some((b) => (b.textContent ?? '').includes(USE))).toBe(true);
	});
});

// ------------------------------------------------------- the count --

/** The live regions of the notices (role="status" or role="alert"), outside
 *  the dialog. */
function notices(container: HTMLElement): HTMLElement[] {
	const all = Array.from(container.querySelectorAll('[role="status"], [role="alert"]')) as HTMLElement[];
	const dialog = container.querySelector('[role="dialog"]');
	return all.filter((n) => !dialog || !dialog.contains(n));
}

describe('the phone’s sync alerts — the histories this session could not read', () => {
	let counted: { count: number; sentence: string | null } = { count: 0, sentence: null };
	let refused = false;

	beforeEach(async () => {
		syncAlerts.alerts = [];
		syncAlerts.previousValues = [];
		syncAlerts.olderDevices = [];
		syncAlerts.newerDevices = [];
		syncAlerts.orphans = [];
		syncAlerts.open = false;
		canvas.threads = [];
		counted = { count: 0, sentence: null };
		refused = false;
		mockTauriCommand('list_threads', () => []);
		mockTauriCommand('canvas_load', () => ({ documents: [], threads: [], relationships: [], milestones: [] }));
		mockTauriCommand('list_previous_values', () => []);
		mockTauriCommand('list_sync_alerts', () => ({ alerts: [], could_not_check: null }));
		mockTauriCommand('list_orphaned_documents', () => ({ orphans: [], could_not_check: null }));
		mockTauriCommand('list_unreadable_documents', () => ({ heading: 'h', meaning: 'm', lines: [] }));
		mockTauriCommand('histories_not_read', () => {
			if (refused) throw LOCKED;
			return counted;
		});
		await reloadSyncAlerts();
		syncAlerts.open = false;
	});

	async function load() {
		await reloadSyncAlerts();
		await settle();
	}

	it('the notice says the count, as a failure is said', async () => {
		const { container } = render(SyncAlerts, { props: { variant: 'mobile' } });
		counted = { count: 1, sentence: ONE_HISTORY };
		await load();
		const said = notices(container).filter((n) => (n.textContent ?? '').includes(ONE_HISTORY));
		expect(said.length, `a live region says “${ONE_HISTORY}”`).toBe(1);
		expect(said[0].getAttribute('role'), 'a failure is an alert').toBe('alert');
	});

	it('says nothing of it when none is counted', async () => {
		const { container } = render(SyncAlerts, { props: { variant: 'mobile' } });
		await load();
		expect(container.textContent ?? '').not.toContain('could not be read');
	});

	it('once none is counted, the sentence goes', async () => {
		const { container } = render(SyncAlerts, { props: { variant: 'mobile' } });
		counted = { count: 1, sentence: ONE_HISTORY };
		await load();
		expect(container.textContent, 'fixture: said first').toContain(ONE_HISTORY);
		counted = { count: 0, sentence: null };
		await load();
		expect(container.textContent).not.toContain(ONE_HISTORY);
	});

	it('a refused call (a locked session) says nothing, and keeps nothing said before', async () => {
		const { container } = render(SyncAlerts, { props: { variant: 'mobile' } });
		counted = { count: 1, sentence: ONE_HISTORY };
		await load();
		expect(container.textContent, 'fixture: said first').toContain(ONE_HISTORY);
		refused = true;
		await load();
		expect(container.textContent, 'what was read under another session never stays on screen').not.toContain(ONE_HISTORY);
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
		expect(files.some((f) => f.endsWith('MobileDocReader.svelte')), 'fixture: the scan sees the reader').toBe(true);
		for (const f of files) {
			const text = readFileSync(f, 'utf8');
			for (const copy of [HISTORY, VERSION, 'The history of', 'The histories of']) {
				expect(text.includes(copy), `${f} holds a copy of “${copy}”`).toBe(false);
			}
		}
	});
});
