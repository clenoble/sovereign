/** C-LOG's host lot on the app, the phone's chat sheet (spec ch. 08, DG3; the Fable maille, by
 *  delegation of Céline, 2026-11-18): below a message that offers it (`offersReadableLog`, set by
 *  the events: `events.quarantinedLog.test.ts`), « Show what remains readable »; it opens the
 *  window of what remains readable, which on the phone offers no Export (Céline, 2026-11-18:
 *  « Mettre l'export du téléphone au backlog »). Written before the code by the lot's test
 *  author; the implementer does not edit this file. */
import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { app } from '$lib/stores/app.svelte';
import { chat } from '$lib/stores/chat.svelte';
import { suggestions } from '$lib/stores/suggestions.svelte';
import MobileChatSheet from './MobileChatSheet.svelte';

const SHOW = 'Show what remains readable';

beforeEach(() => {
	app.bubbleState = 'Idle';
	app.pendingAction = null;
	app.aiName = 'AI';
	chat.messages = [];
	chat.generating = false;
	suggestions.pending = [];
	mockTauriCommand('quarantined_log_readable', () => ({
		entries: ['2026-11-18T21:00:00Z chat_input: READABLEWORDS'],
		after_the_break: 2,
		order_not_guaranteed: false,
		earlier: []
	}));
});

async function sheet() {
	const r = render(MobileChatSheet);
	await tick(); // the auto-expand effect: peek → partial when messages exist
	await tick();
	return r;
}

describe('MobileChatSheet (phone) — the control below the notice or the diagnosed turn', () => {
	it('a message that offers it has the control; it opens the window, with no Export', async () => {
		chat.messages = [
			{ role: 'assistant', text: 'Here is what the check found.', timestamp: 1, offersReadableLog: true }
		];
		const { getByRole, queryByRole, container } = await sheet();
		await fireEvent.click(getByRole('button', { name: SHOW }));
		await waitFor(() => expect(container.ownerDocument.body.textContent).toContain('READABLEWORDS'));
		expect(queryByRole('button', { name: /export/i })).toBeNull();
	});

	it('a message that does not offer it has no control', async () => {
		chat.messages = [{ role: 'assistant', text: 'An answer.', timestamp: 1 }];
		const { queryByRole } = await sheet();
		expect(queryByRole('button', { name: SHOW })).toBeNull();
	});
});
