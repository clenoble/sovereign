/** v0.0.12 F-a11y, F2 on desktop — the chat is announced (WCAG 4.1.3),
 *  `Chat.svelte` (`doc/plans/V0012-frontend-a11y.md`, *F2*). Written before
 *  the code, by another author: whoever implements F2 makes these pass without
 *  editing this file. The phone's twin: `mobile/MobileChatSheet.a11y.svelte.test.ts`.
 *
 *  What the code author provides, in `Chat.svelte`:
 *  - the message list is a `role="log"` element with the accessible name
 *    "Conversation" (`aria-label` or `aria-labelledby`); the person's own
 *    messages are in it, as the others;
 *  - a region that announces (see `$lib/test/announced.ts`: an `aria-live`
 *    polite/assertive element, or a `log`/`status`/`alert` role not set to
 *    `aria-live="off"`), present in the component BEFORE the message arrives, whose
 *    text gains each assistant or system message added after render;
 *  - **[choice of the contract]** the announced text never holds the person's
 *    own message. A live log would read it out, so in practice: the log is
 *    `aria-live="off"` and a separate (visually hidden) `aria-live="polite"`
 *    region carries only the new assistant and system messages. These tests
 *    accept any shape that keeps the three rules;
 *  - nothing announced for the messages present at first render.
 *
 *  - **[choice of the contract, amended 2026-10-25, `7dddd2f`]** a reply or
 *    an outcome line arriving while the panel is hidden IS announced: the
 *    announcing region sits outside the panel's `{#if chat.visible}`.
 *
 *  Readings of the test author, each its own test, named "[reading]"; all
 *  three settled as read in the contract (F2, "Readings settled", `7dddd2f`):
 *  - opening the panel on an existing conversation is a first render of it:
 *    nothing is announced;
 *  - "when the list is restored" is read as the list replaced by one that
 *    does not extend it (`chat.messages = [...]`, other messages): nothing is
 *    announced, since no message was added;
 *  - an addition past the 200 kept for display (`recentMessages()`) is still
 *    an addition: it is announced (a count of the displayed list stops
 *    growing there).
 *
 *  The store is driven through `pushUser` / `pushAssistant` / `pushSystem`
 *  (`$lib/stores/chat.svelte`), as the events do. Positive checks wait (up to
 *  testing-library's default) so a clear-then-set implementation passes;
 *  negative checks let timers run first (`settle`).
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { render, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import Chat from './Chat.svelte';
import { app } from '$lib/stores/app.svelte';
import { chat, pushAssistant, pushSystem, pushUser, type ChatMessage } from '$lib/stores/chat.svelte';
import { announcedText, announcers, settle } from '$lib/test/announced';

const msg = (role: ChatMessage['role'], text: string): ChatMessage => ({ role, text, timestamp: Date.now() });

beforeEach(() => {
	app.pendingAction = null;
	chat.messages = [];
	chat.generating = false;
	chat.visible = true;
});

/** Renders the panel and returns the regions that announce at that moment. */
async function renderPanel() {
	const r = render(Chat);
	await tick();
	await settle();
	const before = announcers(r.container);
	return { ...r, before };
}

/** All the text that could be read out: the regions there before, and now. */
function everythingAnnounced(container: HTMLElement, before: HTMLElement[]): string {
	return announcedText([...new Set([...before, ...announcers(container)])]);
}

describe('Chat (desktop) — F2, the conversation is a named log', () => {
	it('the message list is a log named "Conversation"', async () => {
		const { getByRole } = await renderPanel();
		expect(getByRole('log', { name: 'Conversation' })).toBeTruthy();
	});

	it("the log holds every message, the person's own included", async () => {
		chat.messages = [msg('user', 'kiwi-question-11'), msg('assistant', 'kiwi-answer-12'), msg('system', 'kiwi-note-13')];
		const { getByRole } = await renderPanel();
		const log = getByRole('log', { name: 'Conversation' });
		for (const t of ['kiwi-question-11', 'kiwi-answer-12', 'kiwi-note-13']) {
			expect(within(log).getByText(t, { exact: false }), t).toBeTruthy();
		}
	});
});

describe('Chat (desktop) — F2, what is announced', () => {
	it('an assistant reply pushed after render is announced, by a region already there', async () => {
		const { before } = await renderPanel();
		expect(before.length, 'a region that announces is in the panel before the reply').toBeGreaterThan(0);
		pushAssistant('mango-reply-21');
		await waitFor(() => expect(announcedText(before)).toContain('mango-reply-21'));
	});

	it('a system message pushed after render (an outcome line) is announced', async () => {
		const { before } = await renderPanel();
		expect(before.length, 'a region that announces is in the panel before the message').toBeGreaterThan(0);
		pushSystem('Approved. mango-outcome-22');
		await waitFor(() => expect(announcedText(before)).toContain('mango-outcome-22'));
	});

	it("the person's own message pushed after render is not announced (but is in the log)", async () => {
		const { container, before, getByRole } = await renderPanel();
		pushUser('mango-typed-23');
		await tick();
		await settle();
		expect(within(getByRole('log', { name: 'Conversation' })).getByText('mango-typed-23', { exact: false })).toBeTruthy();
		expect(everythingAnnounced(container, before)).not.toContain('mango-typed-23');
	});

	it("a question then its reply: the reply is announced, the question never", async () => {
		const { container, before } = await renderPanel();
		pushUser('papaya-question-24');
		await tick();
		pushAssistant('papaya-reply-25');
		await waitFor(() => expect(announcedText(before)).toContain('papaya-reply-25'));
		await settle();
		expect(everythingAnnounced(container, before)).not.toContain('papaya-question-24');
	});

	it('nothing is announced for the messages present at first render', async () => {
		chat.messages = [msg('user', 'lime-old-31'), msg('assistant', 'lime-old-32'), msg('system', 'lime-old-33')];
		const { container, before } = await renderPanel();
		const said = everythingAnnounced(container, before);
		for (const t of ['lime-old-31', 'lime-old-32', 'lime-old-33']) expect(said, t).not.toContain(t);
	});

	it('a later reply does not bring the first-render messages into the announcement', async () => {
		chat.messages = [msg('assistant', 'lime-old-34'), msg('system', 'lime-old-35')];
		const { container, before } = await renderPanel();
		pushAssistant('lime-new-36');
		await waitFor(() => expect(announcedText(before)).toContain('lime-new-36'));
		const said = everythingAnnounced(container, before);
		expect(said).not.toContain('lime-old-34');
		expect(said).not.toContain('lime-old-35');
	});

	it('[reading] opening the panel on an existing conversation announces nothing', async () => {
		chat.visible = false;
		chat.messages = [msg('assistant', 'plum-old-41'), msg('system', 'plum-old-42')];
		const { container } = render(Chat);
		await tick();
		chat.visible = true;
		await tick();
		await settle();
		const said = announcedText(announcers(container));
		expect(said).not.toContain('plum-old-41');
		expect(said).not.toContain('plum-old-42');
	});

	it('panel hidden: a reply that arrives is announced, by a region there while hidden', async () => {
		chat.visible = false;
		const { container } = render(Chat);
		await tick();
		await settle();
		const before = announcers(container);
		expect(before.length, 'a region that announces is there with the panel hidden').toBeGreaterThan(0);
		pushAssistant('olive-hidden-reply-81');
		await waitFor(() => expect(announcedText(before)).toContain('olive-hidden-reply-81'));
	});

	it('panel hidden: an outcome line that arrives is announced', async () => {
		chat.visible = false;
		const { container } = render(Chat);
		await tick();
		await settle();
		const before = announcers(container);
		expect(before.length, 'a region that announces is there with the panel hidden').toBeGreaterThan(0);
		pushSystem('Approved. olive-hidden-outcome-82');
		await waitFor(() => expect(announcedText(before)).toContain('olive-hidden-outcome-82'));
	});

	it("panel hidden: the person's own message is still not announced", async () => {
		chat.visible = false;
		const { container } = render(Chat);
		await tick();
		await settle();
		const before = announcers(container);
		pushUser('olive-hidden-typed-83');
		await tick();
		await settle();
		expect(everythingAnnounced(container, before)).not.toContain('olive-hidden-typed-83');
	});

	it('[reading] a list restored (replaced, not extended) announces nothing', async () => {
		chat.messages = [msg('user', 'fig-old-51'), msg('assistant', 'fig-old-52')];
		const { container, before } = await renderPanel();
		chat.messages = [msg('assistant', 'fig-restored-53'), msg('system', 'fig-restored-54')];
		await tick();
		await settle();
		const said = everythingAnnounced(container, before);
		expect(said).not.toContain('fig-restored-53');
		expect(said).not.toContain('fig-restored-54');
	});

	it('[reading] a reply past the 200 messages kept for display is still announced', async () => {
		chat.messages = Array.from({ length: 200 }, (_, i) => msg(i % 2 ? 'assistant' : 'user', `old-${i}`));
		const { before } = await renderPanel();
		expect(before.length).toBeGreaterThan(0);
		pushAssistant('cherry-reply-61');
		await waitFor(() => expect(announcedText(before)).toContain('cherry-reply-61'));
	});
});
