/** Login errors say what failed, on the login screen (spec ch. 07, « Login errors say what
 *  failed — contract (LE) », LE4; Céline, 2026-11-19, through the Fable maille). Written before
 *  the code, by another author: whoever implements the contract makes these pass without editing
 *  this file.
 *
 *  The login screen counts only « Invalid password » toward the lockout; any other error is
 *  shown as the backend gave it and is not counted. An error that arrives with no text never
 *  reads « Invalid password ». What is counted is read where the user reads it: the
 *  « N attempts remaining » line. */
import { beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import LoginScreen from './LoginScreen.svelte';

const NOT_CHECKED =
	'Your password could not be checked: argon2 params: memory cost is too small. This is not a wrong password.';
const RIGHT_NOT_OPENED =
	'Your password is right, but your account could not be opened: the database is locked by another process.';

/** What the next `validate_password` rejects with; `null` resolves (the login opens). */
let nextRejection: unknown;

beforeEach(() => {
	nextRejection = 'Invalid password';
	mockTauriCommand('check_auth_state', () => ({}));
	mockTauriCommand('access_recovery_available', () => false);
	mockTauriCommand('validate_password', () => {
		throw nextRejection;
	});
});

async function settle() {
	for (let i = 0; i < 5; i++) {
		await tick();
		await Promise.resolve();
	}
}

async function loginFailingWith(
	rejection: unknown,
	view: { getByPlaceholderText: (t: string) => HTMLElement; getByRole: (r: string, o: object) => HTMLElement }
) {
	nextRejection = rejection;
	const input = view.getByPlaceholderText('Password');
	await fireEvent.input(input, { target: { value: 'whatever-was-typed' } });
	await fireEvent.click(view.getByRole('button', { name: 'Unlock' }));
	await settle();
}

describe('LoginScreen login errors (LE4)', () => {
	it('an error with no text never reads « Invalid password », and is not counted', async () => {
		const view = render(LoginScreen);
		for (const empty of ['', new Error(''), undefined, {}]) {
			await loginFailingWith(empty, view);
			expect(view.queryByText('Invalid password')).toBeNull();
			expect(view.queryByText(/attempts remaining/)).toBeNull();
		}
	});

	it('« Your password could not be checked: … » is shown as is and is not counted', async () => {
		const view = render(LoginScreen);
		await loginFailingWith(NOT_CHECKED, view);
		expect(view.getByText(NOT_CHECKED)).toBeInTheDocument();
		expect(view.queryByText('Invalid password')).toBeNull();
		expect(view.queryByText(/attempts remaining/)).toBeNull();
	});

	it('« Your password is right, but … » is shown as is and is not counted', async () => {
		const view = render(LoginScreen);
		await loginFailingWith(RIGHT_NOT_OPENED, view);
		expect(view.getByText(RIGHT_NOT_OPENED)).toBeInTheDocument();
		expect(view.queryByText(/attempts remaining/)).toBeNull();
	});

	it('only « Invalid password » counts: a cause that holds the words inside a longer sentence does not', async () => {
		const view = render(LoginScreen);
		const sentence =
			'Your password is right, but your account could not be opened: key database says invalid password format.';
		await loginFailingWith(sentence, view);
		expect(view.getByText(sentence)).toBeInTheDocument();
		expect(view.queryByText(/attempts remaining/)).toBeNull();
	});

	it('« Invalid password » counts, each time', async () => {
		const view = render(LoginScreen);
		await loginFailingWith('Invalid password', view);
		expect(view.getByText('Invalid password')).toBeInTheDocument();
		expect(view.getByText('9 attempts remaining')).toBeInTheDocument();
		await loginFailingWith('Invalid password', view);
		expect(view.getByText('8 attempts remaining')).toBeInTheDocument();
	});

	it('errors that are not a wrong password, between wrong passwords, do not move the count', async () => {
		const view = render(LoginScreen);
		await loginFailingWith('Invalid password', view);
		await loginFailingWith(NOT_CHECKED, view);
		await loginFailingWith(RIGHT_NOT_OPENED, view);
		await loginFailingWith('', view);
		expect(view.getByText('9 attempts remaining')).toBeInTheDocument();
	});
});

describe('LoginScreen login errors (LE6)', () => {
	// A screen reader is told that the login failed, and hears its sentence when it appears,
	// without the person moving to it: the sentence sits in an alert.
	it('each of the three sentences, once shown, is inside an element with role="alert"', async () => {
		for (const sentence of ['Invalid password', NOT_CHECKED, RIGHT_NOT_OPENED]) {
			const view = render(LoginScreen);
			await loginFailingWith(sentence, view);
			const alerts = view.queryAllByRole('alert');
			expect(
				alerts.some((a) => a.textContent?.includes(sentence)),
				`« ${sentence} » is inside an element with role="alert"`
			).toBe(true);
			view.unmount();
		}
	});
});
