/** v0.0.12 F-a11y, F2 on the phone — the chat is announced (WCAG 4.1.3),
 *  `MobileChatSheet.svelte` (`doc/plans/V0012-frontend-a11y.md`, *F2*).
 *  Written before the code, by another author: whoever implements F2 makes
 *  these pass without editing this file. The desktop twin:
 *  `../Chat.a11y.svelte.test.ts`.
 *
 *  What the code author provides, in `MobileChatSheet.svelte`:
 *  - the message list (shown in the partial and full detents) is a
 *    `role="log"` element with the accessible name "Conversation"; the
 *    person's own messages are in it, as the others;
 *  - a region that announces (see `$lib/test/announced.ts`), present in the
 *    sheet BEFORE the message arrives, whose text gains each assistant or
 *    system message added after render;
 *  - **[choice of the contract]** the announced text never holds the person's
 *    own message (in practice: the log `aria-live="off"`, and a separate
 *    visually hidden `aria-live="polite"` region for the new assistant and
 *    system messages);
 *  - nothing announced for the messages present at first render.
 *
 *  Readings of the test author, each its own test, named "[reading]"; all
 *  three settled as read in the contract (F2, "Readings settled", `7dddd2f`).
 *  The sheet has no hidden state (always mounted by `MobileShell`): its
 *  counterpart of the desktop's hidden panel is peek, pinned below:
 *  - the sheet starts in peek with no message, and the list (hence a region
 *    inside it) only appears once the first message has arrived. A live
 *    region inserted together with its text is not reliably read, so the
 *    first message would go unheard: the announcing region must already be
 *    in the sheet in peek, before that message;
 *  - "when the list is restored": the list replaced by one that does not
 *    extend it announces nothing;
 *  - an addition past the 200 kept for display is still announced.
 *
 *  Positive checks wait (testing-library's default) so a clear-then-set
 *  implementation passes; negative checks let timers run first.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { render, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import MobileChatSheet from './MobileChatSheet.svelte';
import { app } from '$lib/stores/app.svelte';
import { chat, pushAssistant, pushSystem, pushUser, type ChatMessage } from '$lib/stores/chat.svelte';
import { suggestions } from '$lib/stores/suggestions.svelte';
import { announcedText, announcers, settle } from '$lib/test/announced';

const msg = (role: ChatMessage['role'], text: string): ChatMessage => ({ role, text, timestamp: Date.now() });

beforeEach(() => {
	app.bubbleState = 'Idle';
	app.pendingAction = null;
	app.aiName = 'AI';
	chat.messages = [];
	chat.generating = false;
	suggestions.pending = [];
	suggestions.visible = false;
});

/** Renders the sheet (expanded when messages exist) and returns the regions
 *  that announce at that moment. */
async function renderSheet() {
	const r = render(MobileChatSheet);
	await tick(); // the auto-expand effect: peek → partial when messages exist
	await tick();
	await settle();
	const before = announcers(r.container);
	return { ...r, before };
}

/** An earlier exchange, so the sheet opens on its list. */
const opening = () => [msg('user', 'hello-start-00')];

function everythingAnnounced(container: HTMLElement, before: HTMLElement[]): string {
	return announcedText([...new Set([...before, ...announcers(container)])]);
}

describe('MobileChatSheet — F2, the conversation is a named log', () => {
	it('the message list is a log named "Conversation"', async () => {
		chat.messages = opening();
		const { getByRole } = await renderSheet();
		expect(getByRole('log', { name: 'Conversation' })).toBeTruthy();
	});

	it("the log holds every message, the person's own included", async () => {
		chat.messages = [msg('user', 'kiwi-question-11'), msg('assistant', 'kiwi-answer-12'), msg('system', 'kiwi-note-13')];
		const { getByRole } = await renderSheet();
		const log = getByRole('log', { name: 'Conversation' });
		for (const t of ['kiwi-question-11', 'kiwi-answer-12', 'kiwi-note-13']) {
			expect(within(log).getByText(t, { exact: false }), t).toBeTruthy();
		}
	});
});

describe('MobileChatSheet — F2, what is announced', () => {
	it('an assistant reply pushed after render is announced, by a region already there', async () => {
		chat.messages = opening();
		const { before } = await renderSheet();
		expect(before.length, 'a region that announces is in the sheet before the reply').toBeGreaterThan(0);
		pushAssistant('mango-reply-21');
		await waitFor(() => expect(announcedText(before)).toContain('mango-reply-21'));
	});

	it('a system message pushed after render (an outcome line) is announced', async () => {
		chat.messages = opening();
		const { before } = await renderSheet();
		expect(before.length, 'a region that announces is in the sheet before the message').toBeGreaterThan(0);
		pushSystem('Approved. mango-outcome-22');
		await waitFor(() => expect(announcedText(before)).toContain('mango-outcome-22'));
	});

	it("the person's own message pushed after render is not announced (but is in the log)", async () => {
		chat.messages = opening();
		const { container, before, getByRole } = await renderSheet();
		pushUser('mango-typed-23');
		await tick();
		await settle();
		expect(within(getByRole('log', { name: 'Conversation' })).getByText('mango-typed-23', { exact: false })).toBeTruthy();
		expect(everythingAnnounced(container, before)).not.toContain('mango-typed-23');
	});

	it('a question then its reply: the reply is announced, the question never', async () => {
		chat.messages = opening();
		const { container, before } = await renderSheet();
		pushUser('papaya-question-24');
		await tick();
		pushAssistant('papaya-reply-25');
		await waitFor(() => expect(announcedText(before)).toContain('papaya-reply-25'));
		await settle();
		expect(everythingAnnounced(container, before)).not.toContain('papaya-question-24');
	});

	it('nothing is announced for the messages present at first render', async () => {
		chat.messages = [msg('user', 'lime-old-31'), msg('assistant', 'lime-old-32'), msg('system', 'lime-old-33')];
		const { container, before } = await renderSheet();
		const said = everythingAnnounced(container, before);
		for (const t of ['lime-old-31', 'lime-old-32', 'lime-old-33']) expect(said, t).not.toContain(t);
	});

	it('a later reply does not bring the first-render messages into the announcement', async () => {
		chat.messages = [msg('assistant', 'lime-old-34'), msg('system', 'lime-old-35')];
		const { container, before } = await renderSheet();
		pushAssistant('lime-new-36');
		await waitFor(() => expect(announcedText(before)).toContain('lime-new-36'));
		const said = everythingAnnounced(container, before);
		expect(said).not.toContain('lime-old-34');
		expect(said).not.toContain('lime-old-35');
	});

	it('[reading] in peek with no message, the region is already there, and the first message is announced', async () => {
		const { before } = await renderSheet();
		expect(before.length, 'a region that announces is in the sheet in peek').toBeGreaterThan(0);
		pushSystem('Proposed: something. melon-first-71');
		await waitFor(() => expect(announcedText(before)).toContain('melon-first-71'));
	});

	it('[reading] a list restored (replaced, not extended) announces nothing', async () => {
		chat.messages = [msg('user', 'fig-old-51'), msg('assistant', 'fig-old-52')];
		const { container, before } = await renderSheet();
		chat.messages = [msg('assistant', 'fig-restored-53'), msg('system', 'fig-restored-54')];
		await tick();
		await settle();
		const said = everythingAnnounced(container, before);
		expect(said).not.toContain('fig-restored-53');
		expect(said).not.toContain('fig-restored-54');
	});

	it('[reading] a reply past the 200 messages kept for display is still announced', async () => {
		chat.messages = Array.from({ length: 200 }, (_, i) => msg(i % 2 ? 'assistant' : 'user', `old-${i}`));
		const { before } = await renderSheet();
		expect(before.length).toBeGreaterThan(0);
		pushAssistant('cherry-reply-61');
		await waitFor(() => expect(announcedText(before)).toContain('cherry-reply-61'));
	});
});
