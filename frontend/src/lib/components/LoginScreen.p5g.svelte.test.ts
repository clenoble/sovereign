/** v0.0.12 P5g — the notice after an interrupted setup (contract `doc/plans/V0012-p5-wizard.md`).
 *
 *  Written from the contract only, before the code, by another author: whoever implements
 *  P5g makes these pass without editing this file.
 *
 *  Mount point: the app's root layout (`routes/+layout.svelte`). The clause says the notice
 *  shows "in the unlocked app", after the login, and where it is rendered is the code
 *  author's choice (the login component, the layout, a notice component). The layout is the
 *  smallest thing that holds both sides of the login: it renders `LoginScreen` while
 *  `app.authState` is 'login' and the unlocked app once it is 'ready'. So the tests render
 *  the layout and read the whole document, wherever the notice ends up inside it.
 *
 *  The backend is mocked at the Tauri `invoke` layer, as a backend that keeps its word:
 *  `check_auth_state` reports `setup_incomplete` until a successful `validate_password`
 *  (of either persona), then no longer (the clause: "the backend clears the state at either
 *  login"). Commands of the unlocked app that the tests do not register throw, as the
 *  shared mock does; the app handles that today.
 *
 *  Most tests run at a phone's width (the wizard of P5 is the phone's); one runs at a desktop
 *  width, since the clause names no surface. */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import { createRawSnippet, tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { app } from '$lib/stores/app.svelte';
import { migrationNotice } from '$lib/stores/migrationNotice.svelte';
import Layout from '../../routes/+layout.svelte';

const NOTICE =
	'Setup was interrupted before it finished. Your account works; your safety phrase, typing profile and sample documents may not have been saved.';

let setupIncomplete: boolean;
let loginOutcome: 'primary' | 'duress' | 'invalid';
let validateCalls: number;

const savedInnerWidth = Object.getOwnPropertyDescriptor(window, 'innerWidth');

function setViewportWidth(w: number) {
	Object.defineProperty(window, 'innerWidth', { get: () => w, configurable: true });
}

beforeEach(() => {
	setupIncomplete = true;
	loginOutcome = 'primary';
	validateCalls = 0;
	app.authState = 'checking';
	migrationNotice.text = null; // its own OK button must not be confused with this one
	setViewportWidth(400); // a phone
	mockTauriCommand('check_auth_state', () => ({
		needs_onboarding: false,
		needs_login: true,
		crypto_enabled: true,
		setup_incomplete: setupIncomplete
	}));
	mockTauriCommand('get_theme', () => 'dark');
	mockTauriCommand('access_recovery_available', () => false);
	mockTauriCommand('validate_password', () => {
		validateCalls++;
		if (loginOutcome === 'invalid') throw 'Invalid password';
		setupIncomplete = false; // the backend clears the marker at either persona's login
		return loginOutcome;
	});
});

afterEach(() => {
	app.authState = 'checking';
	migrationNotice.text = null;
	if (savedInnerWidth) {
		Object.defineProperty(window, 'innerWidth', savedInnerWidth);
	} else {
		delete (window as unknown as { innerWidth?: number }).innerWidth;
	}
});

async function settle() {
	for (let i = 0; i < 10; i++) {
		await tick();
		await Promise.resolve();
	}
}

type Screen = ReturnType<typeof render<typeof Layout>>;

/** The app as it starts: the layout, which asks the backend which gate to show. */
async function launch(): Promise<Screen> {
	const children = createRawSnippet(() => ({ render: () => '<div></div>' }));
	const screen = render(Layout, { props: { children } });
	await waitFor(() => expect(app.authState).toBe('login'));
	await settle();
	expect(screen.getByPlaceholderText('Password')).toBeInTheDocument();
	return screen;
}

async function logIn(screen: Screen, pw = 'Correct!Horse9Battery') {
	await fireEvent.input(screen.getByPlaceholderText('Password'), { target: { value: pw } });
	await fireEvent.click(screen.getByRole('button', { name: 'Unlock' }));
	await settle();
}

/** The document's text, whitespace collapsed, so the notice may span elements or lines. */
function pageText(): string {
	return (document.body.textContent ?? '').replace(/\s+/g, ' ');
}

function noticeCount(): number {
	return pageText().split(NOTICE).length - 1;
}

/** The OK button that belongs to the notice: the nearest one in an ancestor of its text. */
function noticeOkButton(): HTMLButtonElement | null {
	const holders = Array.from(document.body.querySelectorAll<HTMLElement>('*')).filter(
		(el) =>
			(el.textContent ?? '').replace(/\s+/g, ' ').includes(NOTICE) &&
			!Array.from(el.children).some((c) =>
				(c.textContent ?? '').replace(/\s+/g, ' ').includes(NOTICE)
			)
	);
	for (const start of holders) {
		for (let el: HTMLElement | null = start; el && el !== document.body; el = el.parentElement) {
			const ok = Array.from(el.querySelectorAll('button')).find(
				(b) => (b.textContent ?? '').trim() === 'OK' || b.getAttribute('aria-label') === 'OK'
			);
			if (ok) return ok as HTMLButtonElement;
		}
	}
	return null;
}

describe('P5g — the notice after an interrupted setup', () => {
	it('P5g: not shown on the login screen before login, with setup_incomplete true', async () => {
		await launch();
		expect(app.authState).toBe('login');
		expect(noticeCount()).toBe(0);
	});

	it('P5g: shown once in the unlocked app after a successful login, with setup_incomplete true', async () => {
		const screen = await launch();
		await logIn(screen);
		expect(validateCalls).toBe(1);
		await waitFor(() => expect(app.authState).toBe('ready'));
		await waitFor(() => expect(noticeCount()).toBe(1));
		expect(screen.queryByPlaceholderText('Password')).toBeNull(); // the unlocked app, not the login screen
	});

	it('P5g: the notice has an OK button, which dismisses it', async () => {
		const screen = await launch();
		await logIn(screen);
		await waitFor(() => expect(noticeCount()).toBe(1));
		const ok = noticeOkButton();
		expect(ok, 'an OK button with the notice').not.toBeNull();
		await fireEvent.click(ok!);
		await settle();
		expect(noticeCount()).toBe(0);
		expect(app.authState).toBe('ready');
	});

	it('P5g: the same for the duress persona', async () => {
		loginOutcome = 'duress';
		const screen = await launch();
		await logIn(screen);
		await waitFor(() => expect(app.authState).toBe('ready'));
		await waitFor(() => expect(noticeCount()).toBe(1));
	});

	it('P5g: nothing shown after a successful login when setup_incomplete is false', async () => {
		setupIncomplete = false;
		const screen = await launch();
		await logIn(screen);
		await waitFor(() => expect(app.authState).toBe('ready'));
		await settle();
		expect(noticeCount()).toBe(0);
	});

	it('P5g: nothing shown on a failed login', async () => {
		loginOutcome = 'invalid';
		const screen = await launch();
		await logIn(screen);
		expect(validateCalls).toBe(1);
		expect(app.authState).toBe('login');
		expect(screen.getByText('Invalid password')).toBeInTheDocument();
		expect(noticeCount()).toBe(0);
	});

	it('P5g: after a failed login, the first successful login that follows shows it', async () => {
		loginOutcome = 'invalid';
		const screen = await launch();
		await logIn(screen);
		expect(noticeCount()).toBe(0);

		loginOutcome = 'primary';
		await logIn(screen);
		expect(validateCalls).toBe(2);
		await waitFor(() => expect(app.authState).toBe('ready'));
		await waitFor(() => expect(noticeCount()).toBe(1));
	});

	it('P5g: a later login does not show it — the backend no longer reports setup_incomplete', async () => {
		const first = await launch();
		await logIn(first);
		await waitFor(() => expect(noticeCount()).toBe(1));
		await fireEvent.click(noticeOkButton()!);
		await settle();
		first.unmount();

		// The app starts again; the backend cleared the marker at the first login.
		app.authState = 'checking';
		const second = await launch();
		await logIn(second);
		await waitFor(() => expect(app.authState).toBe('ready'));
		await settle();
		expect(noticeCount()).toBe(0);
	});

	it('P5g: a login submitted before the auth check answers still gets the notice', async () => {
		// The app's gate check (made while the app is still 'checking') answers at once, so the
		// login screen opens. Every later check_auth_state call — the login screen's own, or any
		// other — is held until released, and answers with what the backend reported when it was
		// asked: setup_incomplete true, since no login had succeeded yet.
		const held: (() => void)[] = [];
		mockTauriCommand('check_auth_state', () => {
			const answer = {
				needs_onboarding: false,
				needs_login: true,
				crypto_enabled: true,
				setup_incomplete: setupIncomplete
			};
			if (app.authState === 'checking') return answer;
			return new Promise((resolve) => held.push(() => resolve(answer)));
		});

		const screen = await launch();
		// Submitted at once, before any held check answers; then a few ticks pass.
		await logIn(screen);

		// The held checks answer now (with the value of the screen's opening). Nothing about
		// the login is awaited before this: a fix may make the login wait for the screen's own
		// check, or use an earlier answer; both must get here.
		held.splice(0).forEach((release) => release());
		await settle();

		await waitFor(() => expect(app.authState).toBe('ready'));
		expect(validateCalls).toBe(1);
		await waitFor(() => expect(noticeCount()).toBe(1));
	});

	it('P5g: at a desktop width too, shown once after the login and not before', async () => {
		setViewportWidth(1280);
		const screen = await launch();
		expect(noticeCount()).toBe(0);
		await logIn(screen);
		await waitFor(() => expect(app.authState).toBe('ready'));
		await waitFor(() => expect(noticeCount()).toBe(1));
	});
});
