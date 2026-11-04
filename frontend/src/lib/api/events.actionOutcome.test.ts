/**
 * v0.0.12 item 11 (B4, `doc/plans/V0012-lot-assistant.md`): the app shows an
 * executed action's outcome as the one line the backend words
 * (`ActionOutcome::summary_line`), and composes no outcome word itself. Only a
 * real change reads as done; a failure says so once, with no second message.
 *
 * Written at PANDA2's second position on the assistant lot: the lot's tests
 * covered the shell and the payload, not this listener (laptop, claude-coord 0155).
 * The summaries below are what `summary_line` gives for each outcome.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import { subscribeToEvents } from './events';
import { chat } from '$lib/stores/chat.svelte';
import { app } from '$lib/stores/app.svelte';

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

function executed(action: string, success: boolean, summary: string) {
	const h = handlers.get('action-executed');
	expect(h, 'the app listens to action-executed').toBeDefined();
	h!({ payload: { action, success, summary } });
}

describe('action-executed: the outcome line (item 11, B4)', () => {
	it('a change shows the backend line as it is', () => {
		executed('rename_thread', true, '✓ Renamed thread "Work" to "Projects"');
		expect(chat.messages).toHaveLength(1);
		expect(chat.messages[0]).toMatchObject({ role: 'system', text: '✓ Renamed thread "Work" to "Projects"' });
	});

	it('no change never reads as done', () => {
		executed('adopt_document', false, 'No change: the document is already yours');
		expect(chat.messages).toHaveLength(1);
		expect(chat.messages[0].text).toBe('No change: the document is already yours');
		expect(chat.messages[0].text).not.toMatch(/Done|✓/);
	});

	it('a report is shown as reported, not as a change', () => {
		executed('swap_model', true, 'Model switched to qwen2.5-3b');
		expect(chat.messages[0].text).toBe('Model switched to qwen2.5-3b');
		expect(chat.messages[0].text).not.toMatch(/^Done:/);
	});

	it('a failure says why once, with no second message and no "Failed:" prefix', () => {
		executed('delete_thread', false, '✗ Not carried out: the thread was deleted meanwhile');
		expect(chat.messages).toHaveLength(1);
		expect(chat.messages[0].text).toBe('✗ Not carried out: the thread was deleted meanwhile');
		expect(chat.messages[0].text).not.toMatch(/^Failed:/);
	});

	it('the pending action is cleared', () => {
		app.pendingAction = { action: 'rename_thread' } as unknown as typeof app.pendingAction;
		executed('rename_thread', true, '✓ Renamed');
		expect(app.pendingAction).toBeNull();
	});
});
