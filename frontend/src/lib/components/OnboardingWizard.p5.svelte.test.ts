/** v0.0.12 P5 — the phone setup wizard (contract `doc/plans/V0012-p5-wizard.md`).
 *
 *  Written from the contract only, before the code, by another author: whoever implements
 *  P5 makes these pass without editing this file. Where the contract and the code of
 *  b4725dd disagree, the contract wins, so several of these fail until P5 lands.
 *
 *  Same harness as `OnboardingWizard.svelte.test.ts`: the backend is mocked at the Tauri
 *  `invoke` layer (`$lib/test/tauri`); `complete_onboarding` and
 *  `complete_onboarding_paired` return promises the test settles. The component is driven
 *  through the DOM only (fields by label, buttons by role and text). Every comparison in
 *  the contract is on trimmed values, so some tests differ only by surrounding spaces. */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { device } from '$lib/stores/device.svelte';
import { app } from '$lib/stores/app.svelte';
import OnboardingWizard from './OnboardingWizard.svelte';

type Deferred = {
	promise: Promise<unknown>;
	resolve: (v?: unknown) => void;
	reject: (e: unknown) => void;
};
function deferred(): Deferred {
	let resolve!: (v?: unknown) => void;
	let reject!: (e: unknown) => void;
	const promise = new Promise<unknown>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

type Sent = Record<string, unknown>;

let completeCalls: { data: Sent }[];
let pairedCalls: { input: Sent }[];
let pending: Deferred;
let cryptoEnabled: boolean;

const PASSWORD = 'Correct!Horse9Battery';
const DURESS = 'Decoy!Garden7Lantern';
const PHRASE = 'The cat sleeps on warm roofs';
const NO_MATCH = /The two entries don['’]t match\./;
const MUST_DIFFER = /Duress password must differ from your password\./;

const savedMediaDevices = Object.getOwnPropertyDescriptor(navigator, 'mediaDevices');

beforeEach(() => {
	completeCalls = [];
	pairedCalls = [];
	pending = deferred();
	cryptoEnabled = true;
	device.isMobile = true; // the phone's wizard
	app.authState = 'onboarding';
	mockTauriCommand('check_auth_state', () => ({
		needs_onboarding: true,
		needs_login: false,
		crypto_enabled: cryptoEnabled
	}));
	mockTauriCommand('onboarding_designation', () => 'Ikshal-TEST-Ω');
	mockTauriCommand('validate_password_policy', () => ({ valid: true, errors: [] }));
	mockTauriCommand('toggle_theme', () => 'dark');
	mockTauriCommand('complete_onboarding', (args) => {
		completeCalls.push(args as { data: Sent });
		return pending.promise;
	});
	mockTauriCommand('complete_onboarding_paired', (args) => {
		pairedCalls.push(args as { input: Sent });
		return pending.promise;
	});
	mockTauriCommand('consume_pair_qr_preview', () => ({
		source_peer_id: '12D3KooWTest',
		source_device_name: 'Desk',
		issued_at: 0,
		expires_at: 4102444800
	}));
});

afterEach(() => {
	device.isMobile = false;
	app.authState = 'checking';
	if (savedMediaDevices) {
		Object.defineProperty(navigator, 'mediaDevices', savedMediaDevices);
	} else {
		delete (navigator as unknown as { mediaDevices?: unknown }).mediaDevices;
	}
});

async function settle() {
	for (let i = 0; i < 5; i++) {
		await tick();
		await Promise.resolve();
	}
}

type Screen = ReturnType<typeof render<typeof OnboardingWizard>>;

function button(screen: Screen, name: string | RegExp): HTMLButtonElement {
	return screen.getByRole('button', { name }) as HTMLButtonElement;
}

async function next(screen: Screen) {
	await waitFor(() => expect(button(screen, 'Next')).toBeEnabled());
	await fireEvent.click(button(screen, 'Next'));
	await settle();
}

async function type(el: HTMLElement, value: string) {
	await fireEvent.input(el, { target: { value } });
	await settle();
}

async function pressEnter(el: HTMLElement) {
	await fireEvent.keyDown(el, { key: 'Enter' });
	await settle();
}

async function typePasswordPair(screen: Screen, pw = PASSWORD) {
	await type(screen.getByLabelText('Password'), pw);
	await type(screen.getByLabelText('Confirm password'), pw);
}

// Steps, recognised by their heading.
const onDuressStep = (s: Screen) => s.queryByRole('heading', { name: 'Duress password' }) !== null;
const onPhraseStep = (s: Screen) =>
	s.queryByRole('heading', { name: /^(canary|safety) phrase$/i }) !== null;
const onKeystrokeStep = (s: Screen) =>
	s.queryByRole('heading', { name: /keystroke enrollment/i }) !== null;
const onPasswordStep = (s: Screen) =>
	s.queryByRole('heading', { name: 'Set your password' }) !== null;

// Fields of step 6 and step 7.
const duressField = (s: Screen) => s.getByLabelText('Duress password') as HTMLInputElement;
const duressConfirmField = (s: Screen) => s.getByLabelText('Confirm') as HTMLInputElement;
const phraseField = (s: Screen) =>
	s.getByLabelText(/^(canary|safety) phrase$/i) as HTMLInputElement;
const phraseConfirmField = (s: Screen) =>
	s.getByLabelText(/^confirm phrase$/i) as HTMLInputElement;

/** First-device flow, crypto on, to the duress step (6). */
async function toDuressStep(): Promise<Screen> {
	const screen = render(OnboardingWizard);
	await settle();
	await fireEvent.click(screen.getByText('First device'));
	await settle();
	await next(screen); // nickname
	await next(screen); // theme (the bubble step is skipped on mobile)
	await next(screen); // sample data
	await typePasswordPair(screen); // password (5)
	await next(screen);
	await waitFor(() => expect(onDuressStep(screen)).toBe(true));
	return screen;
}

/** From the duress step, with nothing set there, to the safety-phrase step (7). */
async function toPhraseStep(): Promise<Screen> {
	const screen = await toDuressStep();
	await next(screen); // both duress fields empty: Next moves on
	await waitFor(() => expect(onPhraseStep(screen)).toBe(true));
	return screen;
}

/** From step 6 or 7, onward to the last step and Create ("Get Started"). */
async function createFromHere(screen: Screen) {
	for (let i = 0; i < 3 && screen.queryByRole('button', { name: 'Get Started' }) === null; i++) {
		await next(screen);
	}
	await waitFor(() => expect(button(screen, 'Get Started')).toBeEnabled());
	await fireEvent.click(button(screen, 'Get Started'));
	await settle();
}

function sentData(): Sent {
	expect(completeCalls, 'one complete_onboarding call').toHaveLength(1);
	return completeCalls[0].data;
}

// ---------------------------------------------------------------------------
// P5a — Next at the duress step (6)
// ---------------------------------------------------------------------------
describe('P5a — Next at the duress step', () => {
	it('P5a: next enabled when both duress fields are empty, and it moves on as Skip does', async () => {
		const screen = await toDuressStep();
		expect(button(screen, 'Next')).toBeEnabled();
		await fireEvent.click(button(screen, 'Next'));
		await settle();
		expect(onPhraseStep(screen)).toBe(true);
	});

	it('P5a: next enabled when both are non-empty, equal and different from the main password', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), DURESS);
		await type(duressConfirmField(screen), DURESS);
		expect(button(screen, 'Next')).toBeEnabled();
	});

	it('P5a: next enabled when the two entries are equal once trimmed', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), `  ${DURESS}  `);
		await type(duressConfirmField(screen), DURESS);
		expect(button(screen, 'Next')).toBeEnabled();
	});

	it('P5a: next disabled when duress confirmation differs, with "The two entries don\'t match."', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), DURESS);
		await type(duressConfirmField(screen), `${DURESS}x`);
		expect(button(screen, 'Next')).toBeDisabled();
		expect(screen.getByText(NO_MATCH)).toBeInTheDocument();
	});

	it('P5a: next disabled and no mismatch message when only the duress password is filled', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), DURESS);
		expect(button(screen, 'Next')).toBeDisabled();
		expect(screen.queryByText(NO_MATCH)).toBeNull();
	});

	it('P5a: next disabled and no mismatch message when only the duress confirmation is filled', async () => {
		const screen = await toDuressStep();
		await type(duressConfirmField(screen), DURESS);
		expect(button(screen, 'Next')).toBeDisabled();
		expect(screen.queryByText(NO_MATCH)).toBeNull();
	});

	it('P5a: next disabled when the duress password equals the main one, with the desktop\'s words', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), PASSWORD);
		await type(duressConfirmField(screen), PASSWORD);
		expect(button(screen, 'Next')).toBeDisabled();
		expect(screen.getByText(MUST_DIFFER)).toBeInTheDocument();
	});

	it('P5a: next disabled when the duress password equals the main one once trimmed', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), `  ${PASSWORD} `);
		await type(duressConfirmField(screen), ` ${PASSWORD}`);
		expect(button(screen, 'Next')).toBeDisabled();
		expect(screen.getByText(MUST_DIFFER)).toBeInTheDocument();
	});

	it('P5a: Enter on the last duress field does not move on when the entries differ', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), DURESS);
		await type(duressConfirmField(screen), `${DURESS}x`);
		await pressEnter(duressConfirmField(screen));
		expect(onDuressStep(screen)).toBe(true);
	});

	it('P5a: Enter on the last duress field does not move on when it equals the main password', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), PASSWORD);
		await type(duressConfirmField(screen), PASSWORD);
		await pressEnter(duressConfirmField(screen));
		expect(onDuressStep(screen)).toBe(true);
	});
});

// ---------------------------------------------------------------------------
// P5b — Next at the safety-phrase step (7)
// ---------------------------------------------------------------------------
describe('P5b — Next at the safety-phrase step', () => {
	it('P5b: next enabled when both phrase fields are empty', async () => {
		const screen = await toPhraseStep();
		expect(button(screen, 'Next')).toBeEnabled();
		await fireEvent.click(button(screen, 'Next'));
		await settle();
		expect(onKeystrokeStep(screen)).toBe(true);
	});

	it('P5b: next enabled when both are non-empty and equal', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), PHRASE);
		await type(phraseConfirmField(screen), PHRASE);
		expect(button(screen, 'Next')).toBeEnabled();
	});

	it('P5b: next enabled when the two entries are equal once trimmed', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), PHRASE);
		await type(phraseConfirmField(screen), `   ${PHRASE} `);
		expect(button(screen, 'Next')).toBeEnabled();
	});

	it('P5b: next disabled when the phrase confirmation differs, with "The two entries don\'t match."', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), PHRASE);
		await type(phraseConfirmField(screen), 'The cat sleeps on cold roofs');
		expect(button(screen, 'Next')).toBeDisabled();
		expect(screen.getByText(NO_MATCH)).toBeInTheDocument();
	});

	it('P5b: next disabled and no mismatch message when only the phrase is filled', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), PHRASE);
		expect(button(screen, 'Next')).toBeDisabled();
		expect(screen.queryByText(NO_MATCH)).toBeNull();
	});

	it('P5b: next disabled and no mismatch message when only the phrase confirmation is filled', async () => {
		const screen = await toPhraseStep();
		await type(phraseConfirmField(screen), PHRASE);
		expect(button(screen, 'Next')).toBeDisabled();
		expect(screen.queryByText(NO_MATCH)).toBeNull();
	});

	it('P5b: next disabled when a filled, matching phrase is shorter than 4 characters', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), 'abc');
		await type(phraseConfirmField(screen), 'abc');
		expect(button(screen, 'Next')).toBeDisabled();
	});

	it('P5b: next disabled when a matching phrase is shorter than 4 characters once trimmed', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), '  abc  ');
		await type(phraseConfirmField(screen), 'abc   ');
		expect(button(screen, 'Next')).toBeDisabled();
	});

	it('P5b: next enabled when a matching phrase has exactly 4 characters', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), 'abcd');
		await type(phraseConfirmField(screen), 'abcd');
		expect(button(screen, 'Next')).toBeEnabled();
	});

	it('P5b: Enter on the last phrase field does not move on when the entries differ', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), PHRASE);
		await type(phraseConfirmField(screen), 'The cat sleeps on cold roofs');
		await pressEnter(phraseConfirmField(screen));
		expect(onPhraseStep(screen)).toBe(true);
	});
});

// ---------------------------------------------------------------------------
// P5c — Skip sends nothing
// ---------------------------------------------------------------------------
describe('P5c — Skip sends nothing', () => {
	it('P5c: skip at step 6 empties the duress password and its confirmation', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), DURESS);
		await type(duressConfirmField(screen), `${DURESS}x`);
		await fireEvent.click(button(screen, 'Skip this step'));
		await settle();
		expect(onPhraseStep(screen)).toBe(true);

		await fireEvent.click(button(screen, 'Back'));
		await settle();
		expect(onDuressStep(screen)).toBe(true);
		expect(duressField(screen).value).toBe('');
		expect(duressConfirmField(screen).value).toBe('');
	});

	it('P5c: a duress step skipped with values typed sends null for the value and its confirmation', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), DURESS);
		await type(duressConfirmField(screen), `${DURESS}x`);
		await fireEvent.click(button(screen, 'Skip this step'));
		await settle();
		await createFromHere(screen);

		const data = sentData();
		expect(data.duress_password).toBeNull();
		expect(data.duress_confirm).toBeNull();
		pending.resolve();
		await settle();
	});

	it('P5c: skip at step 7 empties the safety phrase and its confirmation', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), PHRASE);
		await type(phraseConfirmField(screen), 'something else');
		await fireEvent.click(button(screen, 'Skip this step'));
		await settle();
		expect(onKeystrokeStep(screen)).toBe(true);

		await fireEvent.click(button(screen, 'Back'));
		await settle();
		expect(onPhraseStep(screen)).toBe(true);
		expect(phraseField(screen).value).toBe('');
		expect(phraseConfirmField(screen).value).toBe('');
	});

	it('P5c: a safety-phrase step skipped with values typed sends null for the value and its confirmation', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), PHRASE);
		await type(phraseConfirmField(screen), PHRASE);
		await fireEvent.click(button(screen, 'Skip this step'));
		await settle();
		await createFromHere(screen);

		const data = sentData();
		expect(data.canary_phrase).toBeNull();
		expect(data.canary_confirm).toBeNull();
		pending.resolve();
		await settle();
	});
});

// ---------------------------------------------------------------------------
// P5d — Create checks again
// ---------------------------------------------------------------------------
describe('P5d — Create checks again', () => {
	it('P5d: main password changed after step 6 to the duress one — nothing is sent, back at step 6 with P5a\'s message', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), DURESS);
		await type(duressConfirmField(screen), DURESS);
		await next(screen); // to 7
		await next(screen); // to 8
		expect(onKeystrokeStep(screen)).toBe(true);

		// Back to the password step, and make the main password the duress one.
		for (let i = 0; i < 3 && !onPasswordStep(screen); i++) {
			await fireEvent.click(button(screen, 'Back'));
			await settle();
		}
		expect(onPasswordStep(screen)).toBe(true);
		await typePasswordPair(screen, DURESS);
		await next(screen);

		// Go as far forward as the wizard lets us, and press Create if it is offered.
		for (let i = 0; i < 4; i++) {
			const create = screen.queryByRole('button', { name: 'Get Started' }) as HTMLButtonElement | null;
			if (create) {
				if (!create.disabled) {
					await fireEvent.click(create);
					await settle();
				}
				break;
			}
			const nextBtn = button(screen, 'Next');
			if (nextBtn.disabled) break;
			await fireEvent.click(nextBtn);
			await settle();
		}

		expect(completeCalls, 'nothing sent').toHaveLength(0);
		await waitFor(() => expect(onDuressStep(screen)).toBe(true));
		expect(screen.getByText(MUST_DIFFER)).toBeInTheDocument();
	});
});

// ---------------------------------------------------------------------------
// P5e — After the account exists, the wizard sends to the login screen
// ---------------------------------------------------------------------------
describe('P5e — after the account exists, the login screen', () => {
	it('P5e: "Already onboarded" from completeOnboarding sets the app to the login screen', async () => {
		const screen = await toDuressStep();
		await createFromHere(screen);
		expect(completeCalls).toHaveLength(1);
		pending.reject('Already onboarded — log in instead');
		await settle();
		await waitFor(() => expect(app.authState).toBe('login'));
	});

	it('P5e: a resolution with { incomplete: true } sets the app to the login screen', async () => {
		const screen = await toDuressStep();
		await createFromHere(screen);
		expect(completeCalls).toHaveLength(1);
		pending.resolve({ incomplete: true });
		await settle();
		await waitFor(() => expect(app.authState).toBe('login'));
	});

	it('P5e: any other resolution keeps today\'s path (authState "ready")', async () => {
		const screen = await toDuressStep();
		await createFromHere(screen);
		expect(completeCalls).toHaveLength(1);
		pending.resolve(undefined);
		await settle();
		await waitFor(() => expect(app.authState).toBe('ready'));
	});

	it('P5e: any other error keeps today\'s on-screen error', async () => {
		const screen = await toDuressStep();
		await createFromHere(screen);
		expect(completeCalls).toHaveLength(1);
		pending.reject('Disk is full');
		await settle();
		await waitFor(() => expect(screen.getByText(/Disk is full/)).toBeInTheDocument());
		expect(app.authState).toBe('onboarding');
	});

	// Amended contract (06252af): this clause wins over the old test, whose error string was
	// changed. The app's layout renders the wizard only while authState is 'onboarding'
	// (routes/+layout.svelte), so leaving it for 'login' is what stops Create being offered;
	// the test asserts that, not a button state inside a wizard the app has unmounted.
	it('P5e: after "Already onboarded" the wizard does not offer Create again', async () => {
		const screen = await toDuressStep();
		await createFromHere(screen);
		expect(completeCalls).toHaveLength(1);
		pending.reject('Already onboarded — log in instead');
		await settle();
		await waitFor(() => expect(app.authState).toBe('login'));
	});
});

// ---------------------------------------------------------------------------
// P5f — The confirmations travel
// ---------------------------------------------------------------------------
describe('P5f — the confirmations travel', () => {
	it('P5f: first device — password_confirm, duress_confirm and canary_confirm are the trimmed confirmations', async () => {
		const screen = await toDuressStep();
		await type(duressField(screen), DURESS);
		await type(duressConfirmField(screen), `  ${DURESS} `);
		await next(screen);
		await type(phraseField(screen), ` ${PHRASE}`);
		await type(phraseConfirmField(screen), `${PHRASE}   `);
		await createFromHere(screen);

		const data = sentData();
		expect(data.password).toBe(PASSWORD);
		expect(data.password_confirm).toBe(PASSWORD);
		expect(data.duress_password).toBe(DURESS);
		expect(data.duress_confirm).toBe(DURESS);
		expect(data.canary_phrase).toBe(PHRASE);
		expect(data.canary_confirm).toBe(PHRASE);
		pending.resolve();
		await settle();
	});

	it('P5f: first device — a confirmation is null exactly when its value is null', async () => {
		const screen = await toDuressStep();
		await createFromHere(screen); // nothing typed at 6 and 7

		const data = sentData();
		expect(data.password).toBe(PASSWORD);
		expect(data.password_confirm).toBe(PASSWORD);
		expect(data.duress_password).toBeNull();
		expect(data.duress_confirm).toBeNull();
		expect(data.canary_phrase).toBeNull();
		expect(data.canary_confirm).toBeNull();
		pending.resolve();
		await settle();
	});

	it('P5f: paired flow — the input carries password_confirm', async () => {
		// A camera that never answers, so the scanner offers its manual-paste button.
		Object.defineProperty(navigator, 'mediaDevices', {
			configurable: true,
			value: { getUserMedia: () => new Promise(() => {}) }
		});
		const screen = render(OnboardingWizard);
		await settle();
		await fireEvent.click(screen.getByText('Pair with existing device'));
		await settle();
		await fireEvent.click(await screen.findByRole('button', { name: 'Use manual paste' }));
		await settle();
		const offer = screen.getByPlaceholderText('paste the offer text from the existing device');
		await fireEvent.input(offer, { target: { value: 'b2ZmZXI=' } });
		await fireEvent.change(offer);
		await settle();
		await fireEvent.input(screen.getByPlaceholderText('XXXXX-XXXXX'), {
			target: { value: 'ABCDE-FGHJK' }
		});
		await settle();
		await next(screen);
		await typePasswordPair(screen);
		await waitFor(() => expect(button(screen, 'Get Started')).toBeEnabled());
		await fireEvent.click(button(screen, 'Get Started'));
		await settle();

		expect(pairedCalls).toHaveLength(1);
		expect(pairedCalls[0].input.password).toBe(PASSWORD);
		expect(pairedCalls[0].input.password_confirm).toBe(PASSWORD);
		pending.resolve();
		await settle();
	});
});

// ---------------------------------------------------------------------------
// Unchanged — without encryption, nothing about passwords is sent
// ---------------------------------------------------------------------------
describe('Unchanged — no encryption', () => {
	it('Unchanged: with cryptoEnabled false, no password, confirmation, duress or phrase is sent', async () => {
		cryptoEnabled = false;
		const screen = render(OnboardingWizard);
		await settle();
		await fireEvent.click(screen.getByText('First device'));
		await settle();
		for (let i = 0; i < 5 && screen.queryByRole('button', { name: 'Get Started' }) === null; i++) {
			await next(screen);
		}
		await waitFor(() => expect(button(screen, 'Get Started')).toBeEnabled());
		await fireEvent.click(button(screen, 'Get Started'));
		await settle();

		const data = sentData();
		for (const key of [
			'password',
			'password_confirm',
			'duress_password',
			'duress_confirm',
			'canary_phrase',
			'canary_confirm'
		]) {
			expect(data[key] ?? null, key).toBeNull();
		}
		pending.resolve();
		await settle();
	});
});

// ---------------------------------------------------------------------------
// K3 — the canary phrase is a secret (Céline, 2026-10-28): entered like a password
// ---------------------------------------------------------------------------
describe('K3 — the canary phrase is entered like a password', () => {
	it('K3: both canary inputs are password inputs, as the password step’s are', async () => {
		const screen = await toPhraseStep();
		expect(phraseField(screen).type, 'Canary phrase').toBe('password');
		expect(phraseConfirmField(screen).type, 'Confirm phrase').toBe('password');
	});

	it('K3: no input on the canary step shows the phrase as text', async () => {
		const screen = await toPhraseStep();
		await type(phraseField(screen), PHRASE);
		await type(phraseConfirmField(screen), PHRASE);
		const visible = Array.from(screen.container.querySelectorAll('input')).filter(
			(el) => (el as HTMLInputElement).type !== 'password' && (el as HTMLInputElement).value.includes(PHRASE)
		);
		expect(visible, 'an input that is not a password input holds the phrase').toHaveLength(0);
	});
});
