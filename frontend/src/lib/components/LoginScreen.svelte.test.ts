/** Typing timings are optional input to login (spec ch. 07, Keystroke dynamics, 2026-10-17).
 *  A key event without a key — what a system autofill dispatches — is ignored: it never ends
 *  up as a sample without `key`, and it never stops the login from being sent. In v0.0.10 such
 *  a sample made the backend refuse the whole call (cloud review, finding 20). */
import { beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import LoginScreen from './LoginScreen.svelte';

type Sample = { key?: unknown; press_ms?: unknown; release_ms?: unknown };
let validateCalls: { password: string; keystrokes: Sample[] }[];

beforeEach(() => {
	validateCalls = [];
	mockTauriCommand('check_auth_state', () => ({}));
	mockTauriCommand('access_recovery_available', () => false);
	mockTauriCommand('validate_password', (args) => {
		validateCalls.push(args as { password: string; keystrokes: Sample[] });
		return 'primary';
	});
});

/** A key event with no `key` property, as autofill produces. */
function keylessEvent(type: 'keydown' | 'keyup'): Event {
	return new Event(type, { bubbles: true, cancelable: true });
}

async function settle() {
	for (let i = 0; i < 5; i++) {
		await tick();
		await Promise.resolve();
	}
}

async function submit(input: HTMLElement, value: string, getByRole: (r: string, o: object) => HTMLElement) {
	await fireEvent.input(input, { target: { value } });
	await fireEvent.click(getByRole('button', { name: 'Unlock' }));
	await settle();
}

function everySampleHasAStringKey(samples: Sample[]): boolean {
	// The samples as they cross the IPC boundary (JSON), where an undefined key disappears.
	const wire = JSON.parse(JSON.stringify(samples)) as Sample[];
	return wire.every((s) => typeof s.key === 'string');
}

describe('LoginScreen typing samples', () => {
	it('drops key-less events and still sends the login', async () => {
		const { getByPlaceholderText, getByRole } = render(LoginScreen);
		const input = getByPlaceholderText('Password');

		input.dispatchEvent(keylessEvent('keydown'));
		input.dispatchEvent(keylessEvent('keyup'));
		await fireEvent.keyDown(input, { key: 'a' });
		await fireEvent.keyUp(input, { key: 'a' });
		input.dispatchEvent(keylessEvent('keydown'));
		input.dispatchEvent(keylessEvent('keyup'));

		await submit(input, 'secret', getByRole);

		expect(validateCalls).toHaveLength(1);
		expect(validateCalls[0].password).toBe('secret');
		expect(everySampleHasAStringKey(validateCalls[0].keystrokes)).toBe(true);
		expect(validateCalls[0].keystrokes.some((s) => s.key === 'a')).toBe(true);
	});

	it('sends the login when the password arrived by autofill alone', async () => {
		const { getByPlaceholderText, getByRole } = render(LoginScreen);
		const input = getByPlaceholderText('Password');

		for (let i = 0; i < 3; i++) {
			input.dispatchEvent(keylessEvent('keydown'));
			input.dispatchEvent(keylessEvent('keyup'));
		}

		await submit(input, 'filled-by-autofill', getByRole);

		expect(validateCalls).toHaveLength(1);
		expect(everySampleHasAStringKey(validateCalls[0].keystrokes)).toBe(true);
	});
});
