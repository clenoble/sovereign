import { beforeEach, describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import MobileChatSheet from './MobileChatSheet.svelte';
import { app } from '$lib/stores/app.svelte';
import { chat, type ChatMessage } from '$lib/stores/chat.svelte';
import { suggestions, type LinkSuggestion } from '$lib/stores/suggestions.svelte';

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeSuggestion(id: string): LinkSuggestion {
	return {
		id,
		fromDocId: 'd:1',
		fromTitle: 'A',
		toDocId: 'd:2',
		toTitle: 'B',
		relationType: 'supports',
		strength: 0.8,
		rationale: 'because',
		source: 'consolidation'
	};
}

function makeMessage(role: ChatMessage['role'], text: string): ChatMessage {
	return { role, text, timestamp: Date.now() };
}

// ── Reset state before each test ─────────────────────────────────────────────

beforeEach(() => {
	app.bubbleState = 'Idle';
	app.pendingAction = null;
	chat.messages = [];
	chat.generating = false;
	suggestions.pending = [];
	suggestions.visible = false;
});

// ── Peek state rendering ─────────────────────────────────────────────────────

describe('MobileChatSheet — peek state', () => {
	it('renders the state orb in peek mode (no messages)', () => {
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.state-orb')).not.toBeNull();
	});

	it('does NOT show the messages list in peek mode', () => {
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.messages')).toBeNull();
	});

	it('does NOT show the input row in peek mode (footer hidden)', () => {
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.input-row')).toBeNull();
	});

	it('shows "Chat with AI" label when there are no messages', () => {
		chat.messages = [];
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.peek-label')?.textContent?.trim()).toBe('Chat with AI');
	});

	it('shows "Thinking…" label when chat.generating is true', () => {
		chat.generating = true;
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.peek-label')?.textContent?.trim()).toBe('Thinking…');
	});

	it('state orb does NOT have .active class when bubbleState is Idle', () => {
		app.bubbleState = 'Idle';
		const { container } = render(MobileChatSheet);
		const orb = container.querySelector('.state-orb') as HTMLElement;
		expect(orb.classList.contains('active')).toBe(false);
	});

	it('state orb has .active class when bubbleState is not Idle', () => {
		app.bubbleState = 'ProcessingOwned';
		const { container } = render(MobileChatSheet);
		const orb = container.querySelector('.state-orb') as HTMLElement;
		expect(orb.classList.contains('active')).toBe(true);
	});

	it('state orb .active updates reactively when bubbleState changes', async () => {
		app.bubbleState = 'Idle';
		const { container } = render(MobileChatSheet);
		const orb = container.querySelector('.state-orb') as HTMLElement;
		expect(orb.classList.contains('active')).toBe(false);

		app.bubbleState = 'Executing';
		await tick();

		expect(orb.classList.contains('active')).toBe(true);
	});
});

// ── Suggestion badge ─────────────────────────────────────────────────────────

describe('MobileChatSheet — suggestion badge', () => {
	it('does not render the badge when suggestions.pending is empty', () => {
		suggestions.pending = [];
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.sugg-badge')).toBeNull();
	});

	it('renders the badge with the correct count', () => {
		suggestions.pending = [makeSuggestion('s:1'), makeSuggestion('s:2'), makeSuggestion('s:3')];
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.sugg-badge')?.textContent?.trim()).toBe('3');
	});

	it('badge appears reactively when suggestions are added', async () => {
		suggestions.pending = [];
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.sugg-badge')).toBeNull();

		suggestions.pending = [makeSuggestion('s:1')];
		await tick();

		expect(container.querySelector('.sugg-badge')).not.toBeNull();
		expect(container.querySelector('.sugg-badge')?.textContent?.trim()).toBe('1');
	});

	it('badge disappears reactively when suggestions are cleared', async () => {
		suggestions.pending = [makeSuggestion('s:1')];
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.sugg-badge')).not.toBeNull();

		suggestions.pending = [];
		await tick();

		expect(container.querySelector('.sugg-badge')).toBeNull();
	});

	it('badge count updates when more suggestions are added', async () => {
		suggestions.pending = [makeSuggestion('s:1')];
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.sugg-badge')?.textContent?.trim()).toBe('1');

		suggestions.pending = [makeSuggestion('s:1'), makeSuggestion('s:2'), makeSuggestion('s:3')];
		await tick();

		expect(container.querySelector('.sugg-badge')?.textContent?.trim()).toBe('3');
	});
});

// ── Pending action quick buttons ─────────────────────────────────────────────

describe('MobileChatSheet — pending action in peek mode', () => {
	it('Approve and Reject buttons are absent with no pendingAction', () => {
		app.pendingAction = null;
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.peek-approve')).toBeNull();
		expect(container.querySelector('.peek-reject')).toBeNull();
	});

	// Compose-level actions do NOT auto-expand, so the peek row stays visible
	// and the quick approve/reject buttons can be tapped without opening the sheet.
	it('shows peek Approve and Reject buttons for Compose-level actions', () => {
		app.pendingAction = { action: 'draft', level: 'Compose', description: 'Draft a message' };
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.peek-approve')).not.toBeNull();
		expect(container.querySelector('.peek-reject')).not.toBeNull();
	});

	it('Approve/Reject buttons appear reactively for a Compose-level action', async () => {
		app.pendingAction = null;
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.peek-approve')).toBeNull();

		app.pendingAction = { action: 'x', level: 'Compose', description: 'Compose something' };
		await tick();

		expect(container.querySelector('.peek-approve')).not.toBeNull();
		expect(container.querySelector('.peek-reject')).not.toBeNull();
	});

	it('Approve/Reject buttons disappear reactively when pendingAction is cleared', async () => {
		app.pendingAction = { action: 'x', level: 'Compose', description: 'x' };
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.peek-approve')).not.toBeNull();

		app.pendingAction = null;
		await tick();

		expect(container.querySelector('.peek-approve')).toBeNull();
		expect(container.querySelector('.peek-reject')).toBeNull();
	});

	// Modify/Destruct-level actions auto-expand to full so the user sees
	// the full action description before confirming — peek buttons are not
	// shown at that gravity level.
	it('Modify-level action auto-expands sheet — peek buttons not shown, action-card shown instead', async () => {
		app.pendingAction = { action: 'delete_doc', level: 'Modify', description: 'Delete "My Doc"' };
		const { container } = render(MobileChatSheet);
		await tick();

		// Sheet expanded — peek row gone, action-card in messages view
		expect(container.querySelector('.peek-approve')).toBeNull();
		expect(container.querySelector('.action-card')).not.toBeNull();
	});
});

// ── Auto-expand on messages ───────────────────────────────────────────────────

describe('MobileChatSheet — auto-expand', () => {
	it('stays in peek when there are no messages (messages list absent)', () => {
		chat.messages = [];
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.peek-row')).not.toBeNull();
		expect(container.querySelector('.messages')).toBeNull();
	});

	it('expands to show messages list when messages exist at mount', async () => {
		chat.messages = [makeMessage('user', 'hello')];
		const { container } = render(MobileChatSheet);
		await tick(); // $effect runs, detent → 'partial'
		expect(container.querySelector('.messages')).not.toBeNull();
		expect(container.querySelector('.peek-row')).toBeNull();
	});

	it('shows the messages list when messages are added after mount', async () => {
		chat.messages = [];
		const { container } = render(MobileChatSheet);
		expect(container.querySelector('.messages')).toBeNull();

		chat.messages = [makeMessage('assistant', 'Hello! How can I help?')];
		await tick();

		expect(container.querySelector('.messages')).not.toBeNull();
	});

	it('renders each message in the list', async () => {
		chat.messages = [
			makeMessage('user', 'first'),
			makeMessage('assistant', 'second'),
			makeMessage('system', 'third')
		];
		const { container } = render(MobileChatSheet);
		await tick();

		const msgs = container.querySelectorAll('.message');
		expect(msgs).toHaveLength(3);
	});

	it('shows the action card when pendingAction is set and in messages view', async () => {
		chat.messages = [makeMessage('user', 'create a doc')];
		app.pendingAction = { action: 'create_doc', level: 'Modify', description: 'Create "My Doc"' };
		const { container } = render(MobileChatSheet);
		await tick();

		expect(container.querySelector('.action-card')).not.toBeNull();
		expect(container.querySelector('.action-card .action-desc')?.textContent?.trim()).toContain(
			'Create "My Doc"'
		);
	});

	it('shows the input row once expanded to partial/full', async () => {
		chat.messages = [makeMessage('user', 'hello')];
		const { container } = render(MobileChatSheet);
		await tick();

		expect(container.querySelector('.input-row')).not.toBeNull();
		const input = container.querySelector('.input-row input') as HTMLInputElement;
		const sendBtn = container.querySelector('.send-btn') as HTMLButtonElement;
		expect(input).not.toBeNull();
		expect(sendBtn).not.toBeNull();
	});
});

// ── Message provenance markers ────────────────────────────────────────────────

describe('MobileChatSheet — message rendering', () => {
	async function renderExpanded(messages: ChatMessage[]) {
		chat.messages = messages;
		const { container } = render(MobileChatSheet);
		await tick();
		return container;
	}

	it('applies .prov-owned class to messages containing "(owned)"', async () => {
		const container = await renderExpanded([
			makeMessage('assistant', 'Here is the document (owned) with your data.')
		]);
		expect(container.querySelector('.message.prov-owned')).not.toBeNull();
	});

	it('applies .prov-external class to messages containing "(external)"', async () => {
		const container = await renderExpanded([
			makeMessage('assistant', 'This content is (external) from the web.')
		]);
		expect(container.querySelector('.message.prov-external')).not.toBeNull();
	});

	it('applies .injection-warning class when text contains "Injection detected"', async () => {
		const container = await renderExpanded([
			makeMessage('system', 'Injection detected in the prompt.')
		]);
		expect(container.querySelector('.injection-warning')).not.toBeNull();
	});

	it('renders user role with "You" prefix', async () => {
		const container = await renderExpanded([makeMessage('user', 'hello')]);
		const prefix = container.querySelector('.message.user .prefix');
		expect(prefix?.textContent?.trim()).toBe('You');
	});

	it('renders assistant role with "AI" prefix', async () => {
		const container = await renderExpanded([makeMessage('assistant', 'hi there')]);
		const prefix = container.querySelector('.message.assistant .prefix');
		expect(prefix?.textContent?.trim()).toBe('AI');
	});
});
