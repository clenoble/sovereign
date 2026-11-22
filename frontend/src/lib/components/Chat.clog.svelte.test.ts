/** C-LOG's host lot on the app, the desktop chat view (spec ch. 08, DG3; the Fable maille, by
 *  delegation of Céline, 2026-11-18): below a message that offers it (`offersReadableLog`, set by
 *  the events: `events.quarantinedLog.test.ts`), « Show what remains readable »; it opens the
 *  window of what remains readable. A message that does not offer it has no control. Written
 *  before the code by the lot's test author; the implementer does not edit this file. */
import { beforeEach, describe, expect, it } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { app } from '$lib/stores/app.svelte';
import { chat } from '$lib/stores/chat.svelte';
import Chat from './Chat.svelte';

const SHOW = 'Show what remains readable';

beforeEach(() => {
	app.pendingAction = null;
	chat.messages = [];
	chat.generating = false;
	chat.visible = true;
	mockTauriCommand('quarantined_log_readable', () => ({
		entries: ['2026-11-18T21:00:00Z chat_input: READABLEWORDS'],
		after_the_break: 2,
		order_not_guaranteed: false,
		earlier: []
	}));
});

describe('Chat (desktop) — the control below the notice or the diagnosed turn', () => {
	it('a message that offers it has the control, and the control opens the window', async () => {
		chat.messages = [
			{ role: 'system', text: "Rejected: session log — Your assistant's conversation log could not be verified", timestamp: 1, offersReadableLog: true }
		];
		const { getByRole, container } = render(Chat);
		await tick();
		await fireEvent.click(getByRole('button', { name: SHOW }));
		await waitFor(() => expect(container.ownerDocument.body.textContent).toContain('READABLEWORDS'));
	});

	it('a message that does not offer it has no control', async () => {
		chat.messages = [{ role: 'assistant', text: 'An answer.', timestamp: 1 }];
		const { queryByRole } = render(Chat);
		await tick();
		expect(queryByRole('button', { name: SHOW })).toBeNull();
	});
});
