/** Item 9 in the browser (spec ch. 03, *Two threads of the name sought: asked
 *  once, remembered*, FC1 and FC2; v0.0.13): « Save to Sovereign » files the
 *  page in the « Web » thread. When two live threads are named « Web » and no
 *  choice holds, the browser asks which one, listing each with its number of
 *  documents and its creation date in local time, in History's format
 *  (`YYYY-MM-DD HH:MM`), and saying the choice will be remembered. Nothing is
 *  saved until she answers; her answer is remembered, then the page is saved
 *  there. No answer: nothing is saved or remembered.
 *
 *  Written before the code by the test author of item 9; the implementer does
 *  not edit this file. One choice per thread is accepted as one button per
 *  thread, or as a radio per thread and then a button that confirms. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import type { ThreadForName } from '$lib/api/commands';

const h = vi.hoisted(() => ({
	answer: { kind: 'create' } as ThreadForName,
	saved: [] as Array<string | undefined>,
	remembered: [] as Array<[string, string]>
}));

vi.mock('$lib/api/commands', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api/commands')>();
	return {
		...actual,
		openBrowser: vi.fn(async () => {}),
		closeBrowserCmd: vi.fn(async () => {}),
		navigateBrowser: vi.fn(async () => {}),
		setBrowserBounds: vi.fn(async () => {}),
		setBrowserVisible: vi.fn(async () => {}),
		extractFormFields: vi.fn(async () => {}),
		threadForName: vi.fn(async () => h.answer),
		rememberThreadChoice: vi.fn(async (name: string, id: string) => {
			h.remembered.push([name, id]);
		}),
		saveWebPage: vi.fn(async (_u: string, _t: string, _c: string, threadId?: string) => {
			h.saved.push(threadId);
			return {};
		})
	};
});

vi.mock('$lib/stores/canvas.svelte', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/stores/canvas.svelte')>();
	return { ...actual, refresh: vi.fn(async () => {}) };
});

import { browser } from '$lib/stores/browser.svelte';
import BrowserPanel from './BrowserPanel.svelte';

const A = { id: 'thread:a', name: 'Web', documents: 2, created_at: '2026-09-01T08:05:00Z' };
const B = { id: 'thread:b', name: 'Web', documents: 0, created_at: '2026-10-15T16:40:00Z' };

/** History's format, in local time. */
function local(iso: string): string {
	const d = new Date(iso);
	const p = (n: number) => String(n).padStart(2, '0');
	return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** The control that picks `t`: the one whose accessible name or label says its date. */
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
	h.answer = { kind: 'create' };
	h.saved = [];
	h.remembered = [];
	browser.isOpen = true;
	browser.url = 'https://example.org/a';
	browser.title = 'A page';
	browser.extractedText = 'Some text of the page';
	browser.reliability = null;
	globalThis.ResizeObserver ??= class {
		observe() {}
		unobserve() {}
		disconnect() {}
	} as unknown as typeof ResizeObserver;
});

async function save() {
	const r = render(BrowserPanel);
	await fireEvent.click(r.getByRole('button', { name: 'Save to Sovereign' }));
	return r;
}

describe('BrowserPanel, saving a page with two « Web » threads', () => {
	it('one « Web », or none: the page is saved, asking nothing', async () => {
		h.answer = { kind: 'use', id: 'thread:only' };
		const r = await save();
		await waitFor(() => expect(h.saved.length).toBe(1));
		expect(document.body.textContent).not.toMatch(/remembered/i);
		r.unmount();
	});

	it('two: asks which, listing each with its documents and its creation date, and saves nothing yet', async () => {
		h.answer = { kind: 'ask', name: 'Web', threads: [A, B] };
		const r = await save();
		await waitFor(() => expect(document.body.textContent).toMatch(/remembered/i));
		const text = document.body.textContent ?? '';
		expect(text).toContain(local(A.created_at));
		expect(text).toContain(local(B.created_at));
		expect(said(control(A)), 'its number of documents, on its row').toMatch(/\b2 documents\b/);
		expect(said(control(B)), 'its number of documents, on its row').toMatch(/\b0 documents\b/);
		expect(h.saved, 'nothing is saved before she answers').toEqual([]);
		expect(h.remembered).toEqual([]);
		r.unmount();
	});

	it('her answer is remembered, then the page is saved in the thread chosen', async () => {
		h.answer = { kind: 'ask', name: 'Web', threads: [A, B] };
		const r = await save();
		await waitFor(() => expect(document.body.textContent).toMatch(/remembered/i));
		await pick(B);
		await waitFor(() => expect(h.saved).toEqual(['thread:b']));
		expect(h.remembered).toEqual([['Web', 'thread:b']]);
		r.unmount();
	});

	it('no answer: nothing is saved and nothing is remembered', async () => {
		h.answer = { kind: 'ask', name: 'Web', threads: [A, B] };
		const r = await save();
		await waitFor(() => expect(document.body.textContent).toMatch(/remembered/i));
		await fireEvent.click(r.getByRole('button', { name: /cancel/i }));
		await new Promise((res) => setTimeout(res, 20));
		expect(h.saved).toEqual([]);
		expect(h.remembered).toEqual([]);
		r.unmount();
	});
});
