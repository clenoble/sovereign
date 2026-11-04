/**
 * v0.0.12 rc2 — a summary is shown in the app (contract R3, with R4's
 * malformed case; `doc/plans/V0012-rc2-summary-shown.md`). Found by Céline on
 * rc1 (checklist §22, 2026-11-01): the app showed a summary as
 * `Skill "summarizer": {"doc_title":…}`, the raw JSON.
 *
 * Written before the code, by another author: whoever fixes the listener makes
 * these pass without editing this file. The payload below is what
 * `produce_summary` (`sovereign-ai/src/orchestrator.rs`) sends, forwarded by
 * `sovereign-app/src/tauri_events.rs` as `skill-result`.
 *
 * Readings chosen: the text is checked for the title, the summary unchanged,
 * the part count ("<N> parts") when N > 1, and no JSON; the punctuation and the
 * message's role are free. R4's plain line for malformed data is read as
 * holding in the app too (R4 does not name a host).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import { subscribeToEvents } from './events';
import { chat } from '$lib/stores/chat.svelte';

type Handler = (e: { payload: unknown }) => void;
const handlers = new Map<string, Handler>();

beforeEach(async () => {
	handlers.clear();
	vi.mocked(listen).mockImplementation((async (name: string, h: Handler) => {
		handlers.set(name, h);
		return () => {};
	}) as unknown as typeof listen);
	chat.messages = [];
	await subscribeToEvents();
});

function skillResult(skill: string, action: string, kind: string, data: string) {
	const h = handlers.get('skill-result');
	expect(h, 'the app listens to skill-result').toBeDefined();
	h!({ payload: { skill, action, kind, data } });
}

function summary(doc_title: string, text: string, parts: number) {
	skillResult('summarizer', 'summarize', 'summary', JSON.stringify({ doc_title, summary: text, parts }));
}

/** A summary as the model gives it, PII already resolved: two lines, non-ASCII. */
const SUMMARY =
	"Le texte décrit la réunion du 3 mars avec [Person 1] : budget révisé de 12 %.\nDeux décisions — reporter l'essai, écrire au fournisseur.";

describe('skill-result: a summary is shown as text (R3)', () => {
	it('R3: a one-part summary shows the title and the summary unchanged, no JSON', () => {
		summary('Texte A', SUMMARY, 1);
		expect(chat.messages).toHaveLength(1);
		const text = chat.messages[0].text;
		expect(text).toContain('Texte A');
		expect(text).toContain(SUMMARY);
		expect(text).not.toContain('{');
		expect(text).not.toContain('doc_title');
		expect(text).not.toMatch(/^Skill "/);
		expect(text).not.toContain('1 part');
	});

	it('R3: a summary in parts says how many', () => {
		summary('Texte B', SUMMARY, 4);
		expect(chat.messages).toHaveLength(1);
		const text = chat.messages[0].text;
		expect(text).toContain('Texte B');
		expect(text).toContain(SUMMARY);
		expect(text).toContain('4 parts');
		expect(text).not.toContain('{');
		expect(text).not.toContain('doc_title');
	});

	it('R3: a model_list result is shown as it is today', () => {
		const data = JSON.stringify({ models: [{ name: 'qwen2.5-3b.gguf', size_mb: 1900 }] });
		skillResult('model_manager', 'list_models', 'model_list', data);
		expect(chat.messages).toHaveLength(1);
		expect(chat.messages[0].text).toBe(`Skill "model_manager": ${data}`);
	});

	it('R3 (with R4): unreadable summary data shows one plain line, never the raw text', () => {
		skillResult('summarizer', 'summarize', 'summary', 'zebra-not-json <<summary>>');
		expect(chat.messages).toHaveLength(1);
		expect(chat.messages[0].text).toContain('The summary could not be shown');
		expect(chat.messages[0].text).not.toContain('zebra-not-json');
	});

	it('R3 (with R4): summary data without a summary shows one plain line, never the raw text', () => {
		skillResult('summarizer', 'summarize', 'summary', JSON.stringify({ doc_title: 'zebra-title', parts: 2 }));
		expect(chat.messages).toHaveLength(1);
		expect(chat.messages[0].text).toContain('The summary could not be shown');
		expect(chat.messages[0].text).not.toContain('{');
		expect(chat.messages[0].text).not.toContain('doc_title');
	});
});
