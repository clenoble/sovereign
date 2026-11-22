/** C-LOG's host lot on the app (spec ch. 08, DG3; the Fable maille, by delegation of Céline,
 *  2026-11-18): « Show what remains readable » is offered below the quarantine notice and below
 *  the assistant turn that ends after the diagnosis tool found a quarantine. The events mark
 *  those chat messages (`offersReadableLog`); the chat views draw the control
 *  (`Chat.clog.svelte.test.ts`, `mobile/MobileChatSheet.clog.svelte.test.ts`).
 *
 *  The names are the app's (`tauri_events.rs`): the notice comes as `action-rejected` with the
 *  action « session log »; the diagnosis as `quarantined-log-diagnosed`, before the turn's reply.
 *  Written before the code by the lot's test author; the implementer does not edit this file. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import { subscribeToEvents } from './events';
import { chat } from '$lib/stores/chat.svelte';

type Handler = (e: { payload: unknown }) => void;
const handlers = new Map<string, Handler>();

/** The quarantine notice's first words (Céline's, spec ch. 08, *Session log*). */
const NOTICE =
	"Your assistant's conversation log could not be verified: it may have been changed while Sovereign was closed, " +
	'or a write was cut short (a crash, a full disk). The assistant starts without its earlier conversations. ' +
	"Nothing was deleted. Ask the assistant 'what happened to my log?'";

beforeEach(async () => {
	handlers.clear();
	vi.mocked(listen).mockImplementation((async (name: string, h: Handler) => {
		handlers.set(name, h);
		return () => {};
	}) as unknown as typeof listen);
	chat.messages = [];
	await subscribeToEvents();
});

function fire(name: string, payload: unknown = null) {
	const h = handlers.get(name);
	expect(h, `the app listens to ${name}`).toBeDefined();
	h!({ payload });
}

describe('the control « Show what remains readable » — which messages offer it', () => {
	it('the quarantine notice offers it', () => {
		fire('action-rejected', { action: 'session log', reason: NOTICE });
		const last = chat.messages.at(-1);
		expect(last?.text).toContain('could not be verified');
		expect(last?.offersReadableLog).toBe(true);
	});

	it('another rejection does not', () => {
		fire('action-rejected', { action: 'delete document', reason: 'Not found.' });
		expect(chat.messages.at(-1)?.offersReadableLog).not.toBe(true);
	});

	it('the turn that ends after the diagnosis offers it, and the next turn does not', () => {
		fire('quarantined-log-diagnosed');
		fire('chat-response', { text: 'Here is what the check found.' });
		const diagnosed = chat.messages.filter((m) => m.role === 'assistant');
		expect(diagnosed.at(-1)?.offersReadableLog).toBe(true);
		fire('chat-response', { text: 'Anything else?' });
		const next = chat.messages.filter((m) => m.role === 'assistant');
		expect(next.at(-1)?.offersReadableLog).not.toBe(true);
	});

	it('a turn without the diagnosis does not', () => {
		fire('chat-response', { text: 'An answer.' });
		expect(chat.messages.at(-1)?.offersReadableLog).not.toBe(true);
	});
});
