/** History's names on the phone (spec ch. 12, HN1-HN4; v0.0.13, the scope's C5 line « device
 *  names, not the technical id »). Written before the code by its test author; the implementer
 *  does not edit this file.
 *
 *  A row shows the words `list_commits` gives (`shown_message`, `local_time`), never the stored
 *  message's peer id or commit id, nor the raw time. Restoring the version that is already the
 *  current one says « This is already the current version. Nothing was changed. », the sentence
 *  `restore_commit` gives (`already_current`); a restore that changes the document says nothing
 *  of it. */
import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, render } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { openById, panels } from '$lib/stores/documents.svelte';
import MobileDocReader from './MobileDocReader.svelte';

const PEER = '12D3KooWQnB9n6Hk1vV4Y8q2sWm3Yt7Zr5Xc1Pa9Lb4Kd6Ef8Gh';
const COMMIT = 'commit:x7k2m9q4w1e8r5t3';
const ALREADY_CURRENT = 'This is already the current version. Nothing was changed.';
const USE = 'Use this version';

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

const rows = () => ({
	commits: [
		{
			id: 'commit:newer',
			message: `Restored from ${COMMIT}`,
			timestamp: '2026-11-18T09:30:00Z',
			snapshot_title: 'Notes',
			snapshot_preview: 'some text',
			could_not_read: null,
			shown_message: 'Restored from the version of 2026-11-18 10:00',
			local_time: '2026-11-18 10:30'
		},
		{
			id: 'commit:older',
			message: `Before sync from ${PEER}`,
			timestamp: '2026-11-18T09:00:00Z',
			snapshot_title: 'Notes',
			snapshot_preview: 'earlier text',
			could_not_read: null,
			shown_message: 'Before sync from Pixel of hers',
			local_time: '2026-11-18 10:00'
		}
	],
	could_not_read: null
});

async function settle() {
	for (let i = 0; i < 5; i++) {
		await tick();
		await new Promise((resolve) => setTimeout(resolve, 0));
	}
}

/** Opens a fresh document in the reader and turns to its History. */
async function history(restore?: (args: unknown) => unknown) {
	const id = `document:hn-${++seq}`;
	mockTauriCommand('get_document', () => doc(id));
	mockTauriCommand('get_document_labelled', () => ({ title: 'Notes', body: 'some text' }));
	mockTauriCommand('list_commits', rows);
	if (restore) mockTauriCommand('restore_commit', restore);
	await openById(id);
	const panel = panels.find((p) => p.doc.id === id);
	expect(panel, 'fixture: the document opens').toBeTruthy();
	const r = render(MobileDocReader, { props: { panel: panel! } });
	await fireEvent.click(r.getByRole('button', { name: 'History' }));
	await settle();
	return { ...r, id };
}

/** Everything the reader gives to the eye or to a screen reader. */
function said(container: HTMLElement): string {
	const labels = [...container.querySelectorAll('[aria-label]')].map((e) => e.getAttribute('aria-label'));
	return `${container.textContent ?? ''}\n${labels.join('\n')}`;
}

beforeEach(() => {
	panels.splice(0, panels.length);
});

describe('the phone’s History — names, not ids (HN1, HN2)', () => {
	it('each row says the words the backend gives, and its local time', async () => {
		const { container } = await history();
		const text = said(container);
		expect(text).toContain('Before sync from Pixel of hers');
		expect(text).toContain('Restored from the version of 2026-11-18 10:00');
		expect(text).toContain('2026-11-18 10:30');
		expect(text).toContain('2026-11-18 10:00');
	});

	it('never the peer id, the commit id, or the raw time', async () => {
		const { container } = await history();
		const text = said(container);
		expect(text).not.toContain(PEER);
		expect(text).not.toContain(COMMIT);
		expect(text).not.toContain('x7k2m9q4w1e8r5t3');
		expect(text).not.toContain('2026-11-18T09:30:00Z');
	});
});

describe('the phone’s History — restoring the current version (HN4)', () => {
	async function use(alreadyCurrent: string | null) {
		const h = await history(() => ({ document: doc('document:restored'), already_current: alreadyCurrent }));
		await fireEvent.click(h.getByText('Before sync from Pixel of hers'));
		await settle();
		await fireEvent.click(h.getByRole('button', { name: USE }));
		await settle();
		return h;
	}

	it('says the sentence when the version was already the current one', async () => {
		const { container } = await use(ALREADY_CURRENT);
		expect(said(container)).toContain(ALREADY_CURRENT);
	});

	it('says nothing of it when the restore changed the document', async () => {
		const { container } = await use(null);
		expect(said(container)).not.toContain(ALREADY_CURRENT);
	});
});
