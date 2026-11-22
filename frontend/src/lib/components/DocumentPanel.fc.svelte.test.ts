/** Item 9 through a skill run from a document (spec ch. 03, *Two threads of the name sought: asked
 *  once, remembered*, FC1 and FC2; v0.0.13; the Fable maille, 2026-11-18: the document panel asks
 *  as the skills panel does). The journal skill is offered for every document, so « Open Today's
 *  Journal » can be pressed from the document panel. When two live « Journal » threads exist and
 *  no choice holds, the skill answers `ask_thread`, having written nothing: the panel asks which,
 *  with the same window as the skills panel (`SkillsPanel.fc.svelte.test.ts`); her answer is
 *  remembered and the skill runs again; no answer, nothing is remembered and it is not run again.
 *  Until now the panel let that answer fall through: no window, no line.
 *
 *  Written before the code by the test author of item 9; the implementer does not edit this
 *  file. One choice per thread is accepted as one button per thread, or as a radio per thread and
 *  then a button that confirms. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import type { OpenPanel } from '$lib/stores/documents.svelte';
import type { SkillResultDto, ThreadForName } from '$lib/api/commands';

const A = { id: 'thread:a', name: 'Journal', documents: 12, created_at: '2026-08-03T07:30:00Z' };
const B = { id: 'thread:b', name: 'Journal', documents: 1, created_at: '2026-11-02T21:10:00Z' };
const QUESTION: ThreadForName = { kind: 'ask', name: 'Journal', threads: [A, B] };
const DOC = 'document:open';

const h = vi.hoisted(() => ({
	runs: [] as Array<[string, string, string]>,
	remembered: [] as Array<[string, string]>,
	answers: [] as unknown[]
}));

function result(kind: string, json: string | null): SkillResultDto {
	return {
		kind,
		body: null,
		images: null,
		videos: null,
		file_name: null,
		file_mime: null,
		file_data_base64: null,
		structured_kind: kind === 'ask_thread' ? 'ask_thread' : 'journal_entry',
		structured_json: json
	} as unknown as SkillResultDto;
}

vi.mock('$lib/api/commands', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api/commands')>();
	return {
		...actual,
		listSkillsForDoc: vi.fn(async () => [
			{ skill_name: 'daily-journal', actions: [{ action_id: 'today', label: "Open Today's Journal" }] }
		]),
		executeSkill: vi.fn(async (skill: string, action: string, docId: string) => {
			h.runs.push([skill, action, docId]);
			return h.answers.shift();
		}),
		rememberThreadChoice: vi.fn(async (name: string, id: string) => {
			h.remembered.push([name, id]);
		})
	};
});

import DocumentPanel from './DocumentPanel.svelte';

function panel(): OpenPanel {
	return {
		doc: {
			id: DOC,
			title: 'Notes',
			body: 'some text',
			images: [],
			videos: [],
			thread_id: 'thread:t',
			is_owned: true,
			created_at: '2026-11-18T08:00:00Z',
			modified_at: '2026-11-18T09:00:00Z',
			revision: 'r1'
		},
		dirty: false,
		position: { x: 0, y: 0 },
		size: { width: 800, height: 600 },
		zIndex: 100,
		mode: 'edit',
		commits: [],
		commitsLoaded: false,
		selectedCommit: null,
		skillsOverflowOpen: false,
		saveError: null,
		saveInfo: null,
		conflict: false
	} as unknown as OpenPanel;
}

function local(iso: string): string {
	const d = new Date(iso);
	const p = (n: number) => String(n).padStart(2, '0');
	return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** What a control says: its accessible name, its text, its label's text. */
function said(e: HTMLElement): string {
	return `${e.getAttribute('aria-label') ?? ''} ${e.textContent ?? ''} ${e.closest('label')?.textContent ?? ''}`;
}

function control(t: typeof A): HTMLElement {
	const all = [...screen.queryAllByRole('button'), ...screen.queryAllByRole('radio')];
	const found = all.find((e) => said(e).includes(local(t.created_at)));
	if (!found) throw new Error(`no control names ${local(t.created_at)}`);
	return found;
}

async function pick(t: typeof A) {
	await fireEvent.click(control(t));
	if (h.remembered.length === 0) {
		const confirm = screen.queryAllByRole('button').find((b) => /^(use|choose|ok|confirm)\b/i.test(b.textContent?.trim() ?? ''));
		if (confirm) await fireEvent.click(confirm);
	}
}

beforeEach(() => {
	h.runs = [];
	h.remembered = [];
	h.answers = [];
});

async function runJournal() {
	const r = render(DocumentPanel, { props: { panel: panel() } });
	await fireEvent.click(await r.findByRole('button', { name: "Open Today's Journal" }));
	return r;
}

describe('DocumentPanel, the journal with two « Journal » threads', () => {
	it('asks which, listing each with its documents and its creation date, and runs nothing more', async () => {
		h.answers = [result('ask_thread', JSON.stringify(QUESTION))];
		const r = await runJournal();
		await waitFor(() => expect(document.body.textContent).toMatch(/remembered/i));
		const text = document.body.textContent ?? '';
		expect(text).toContain(local(A.created_at));
		expect(text).toContain(local(B.created_at));
		expect(said(control(A)), 'its number of documents, on its row').toMatch(/\b12 documents\b/);
		expect(said(control(B)), 'its number of documents, on its row').toMatch(/\b1 document\b/);
		expect(h.runs.length, 'the skill is not run again before she answers').toBe(1);
		expect(h.remembered).toEqual([]);
		r.unmount();
	});

	it('her answer is remembered, then the skill runs again on the same document', async () => {
		h.answers = [
			result('ask_thread', JSON.stringify(QUESTION)),
			result('structured_data', JSON.stringify({ doc_id: 'document:j', thread_id: 'thread:a', created: true }))
		];
		const r = await runJournal();
		await waitFor(() => expect(document.body.textContent).toMatch(/remembered/i));
		await pick(A);
		await waitFor(() => expect(h.runs.length).toBe(2));
		expect(h.remembered).toEqual([['Journal', 'thread:a']]);
		expect(h.runs[1], 'the same skill, action and document').toEqual(h.runs[0]);
		expect(h.runs[0][2]).toBe(DOC);
		r.unmount();
	});

	it('no answer: nothing is remembered and the skill is not run again', async () => {
		h.answers = [result('ask_thread', JSON.stringify(QUESTION))];
		const r = await runJournal();
		await waitFor(() => expect(document.body.textContent).toMatch(/remembered/i));
		await fireEvent.click(r.getByRole('button', { name: /cancel/i }));
		await new Promise((res) => setTimeout(res, 20));
		expect(h.runs.length).toBe(1);
		expect(h.remembered).toEqual([]);
		r.unmount();
	});
});
