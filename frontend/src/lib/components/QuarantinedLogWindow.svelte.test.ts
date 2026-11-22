/** C-LOG's host lot on the app (spec ch. 08, DG3 and DG4; the Fable maille, by delegation of
 *  Céline, 2026-11-18, on the window's words; Céline, 2026-11-18: « Mettre l'export du téléphone
 *  au backlog »). Written before the code by the lot's test author; the implementer does not edit
 *  this file.
 *
 *  The window shows what `quarantined_log_readable` gives: the entries as plain text, the count
 *  of those not shown in the decided words, the older format's sentence, the earlier quarantines.
 *  It offers « Keep it » and « Delete it… », and no Export. Delete asks first, the focus on
 *  « Cancel »; only « Delete » calls `delete_quarantined_log` with `confirmed: true`. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { chat } from '$lib/stores/chat.svelte';
import QuarantinedLogWindow from './QuarantinedLogWindow.svelte';
import source from './QuarantinedLogWindow.svelte?raw';

const COUNT_TWO = '2 entries are not shown: from the break on, they no longer verify.';
const COUNT_ONE = '1 entry is not shown: from the break on, it no longer verifies.';
const OLDER = 'These entries come from the older format: their order and completeness are not guaranteed.';
const DELETE_QUESTION = 'Delete this quarantined log? It cannot be brought back.';
const DELETED = 'The quarantined log was deleted.';

const ENTRY = '2026-11-18T21:00:00Z chat_input: READABLEWORDS [U+202E]reversed <a href="https://x.example">x</a>';

type Readable = { entries: string[]; after_the_break: number; order_not_guaranteed: boolean; earlier: string[] };

let deletes: unknown[];

function serve(r: Readable | null) {
	mockTauriCommand('quarantined_log_readable', () => r);
	mockTauriCommand('delete_quarantined_log', (args: unknown) => {
		deletes.push(args);
		return null;
	});
}

const readable = (over: Partial<Readable> = {}): Readable => ({
	entries: [ENTRY],
	after_the_break: 2,
	order_not_guaranteed: false,
	earlier: [],
	...over
});

async function shown(r: Readable) {
	serve(r);
	const onclose = vi.fn();
	const view = render(QuarantinedLogWindow, { props: { onclose } });
	await waitFor(() => expect(view.container.textContent).toContain('READABLEWORDS'));
	return { ...view, onclose };
}

beforeEach(() => {
	deletes = [];
	chat.messages = [];
});

describe('QuarantinedLogWindow — DG3, what remains readable', () => {
	it('shows each entry as plain text: its markup is text, never an element', async () => {
		const { container } = await shown(readable());
		expect(container.textContent).toContain(ENTRY);
		expect(container.querySelector('a')).toBeNull();
	});

	it('counts two entries not shown in the decided words', async () => {
		const { container } = await shown(readable());
		expect(container.textContent).toContain(COUNT_TWO);
	});

	it('counts one entry not shown in the decided words', async () => {
		const { container } = await shown(readable({ after_the_break: 1 }));
		expect(container.textContent).toContain(COUNT_ONE);
	});

	it('says no count when every entry is shown', async () => {
		const { container } = await shown(readable({ after_the_break: 0 }));
		expect(container.textContent).not.toMatch(/not shown/);
	});

	it('says the older format is not guaranteed, only for a log in it', async () => {
		const older = await shown(readable({ order_not_guaranteed: true }));
		expect(older.container.textContent).toContain(OLDER);
		older.unmount();
		const newer = await shown(readable());
		expect(newer.container.textContent).not.toContain(OLDER);
	});

	it('names the earlier quarantines', async () => {
		const earlier = 'An earlier log was also quarantined: number 1, on 2026-11-17 09:12.';
		const { container } = await shown(readable({ earlier: [earlier] }));
		expect(container.textContent).toContain(earlier);
	});

	it('its source renders no markup and makes no link', () => {
		expect(source).not.toContain('{@html');
		expect(source).not.toMatch(/<a[\s>]/);
	});
});

describe('QuarantinedLogWindow — DG4, Keep and Delete, no Export', () => {
	it('offers « Keep it » and « Delete it… », and no Export', async () => {
		const { getByRole, queryByRole } = await shown(readable());
		expect(getByRole('button', { name: 'Keep it' })).toBeTruthy();
		expect(getByRole('button', { name: /^Delete it/ })).toBeTruthy();
		expect(queryByRole('button', { name: /export/i })).toBeNull();
	});

	it('« Keep it » closes the window and deletes nothing', async () => {
		const { getByRole, onclose } = await shown(readable());
		await fireEvent.click(getByRole('button', { name: 'Keep it' }));
		await tick();
		expect(onclose).toHaveBeenCalled();
		expect(deletes).toHaveLength(0);
	});

	it('« Delete it… » asks first, the focus on « Cancel »; « Cancel » deletes nothing', async () => {
		const { getByRole, container } = await shown(readable());
		await fireEvent.click(getByRole('button', { name: /^Delete it/ }));
		await tick();
		expect(container.textContent).toContain(DELETE_QUESTION);
		const cancel = getByRole('button', { name: 'Cancel' });
		expect(getByRole('button', { name: 'Delete' })).toBeTruthy();
		await waitFor(() => expect(document.activeElement).toBe(cancel));
		expect(deletes).toHaveLength(0);
		await fireEvent.click(cancel);
		await tick();
		expect(deletes).toHaveLength(0);
		expect(container.textContent).not.toContain(DELETE_QUESTION);
	});

	it('« Delete » deletes, confirmed, once; says so in the chat; and closes the window', async () => {
		const { getByRole, onclose } = await shown(readable());
		await fireEvent.click(getByRole('button', { name: /^Delete it/ }));
		await tick();
		await fireEvent.click(getByRole('button', { name: 'Delete' }));
		await waitFor(() => expect(deletes).toHaveLength(1));
		expect(deletes[0]).toEqual({ confirmed: true });
		await waitFor(() => expect(chat.messages.some((m) => m.text.includes(DELETED))).toBe(true));
		expect(onclose).toHaveBeenCalled();
	});
});
