/** v0.0.13 C9 — what the app wizard's secret inputs say to the system's password manager
 *  (the scope of item C9 in v0.0.13, 2026-11-17):
 *  - the password step's two inputs: `new-password`, so a password manager may offer to save the
 *    main password (Céline, 2026-10-28: « oui, mais plus tard », on a line that already said
 *    `new-password` for the main password);
 *  - the paired setup's two password inputs: the same, since they set the same password, with the
 *    password step's other attributes (`autocapitalize` and `autocorrect` "off", `spellcheck`
 *    "false"), as K5 gives them on the duress and canary steps;
 *  - the keystroke sample: `off`, so no manager offers to fill what must be typed (a filled sample
 *    is ignored at login, ch. 07; `off` keeps it from being offered at all).
 *  - the login's password input: `current-password`, so the manager fills it back (Céline,
 *    2026-11-17: « Oui, le remplir »).
 *  Spec ch. 12, K6 (v2.0.109).
 *
 *  Written before the code, by another author than whoever implements it (the laptop maille, as
 *  C9's test author): whoever implements makes these pass without editing this file. A test that
 *  looks wrong is reported, never worked around.
 *
 *  Same harness as `OnboardingWizard.p5.svelte.test.ts` and `OnboardingWizard.show.svelte.test.ts`:
 *  the backend is mocked at the Tauri `invoke` layer, and the component is driven through the DOM
 *  only (fields by label or placeholder, buttons by role and text). */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { device } from '$lib/stores/device.svelte';
import { app } from '$lib/stores/app.svelte';
import OnboardingWizard from './OnboardingWizard.svelte';
import LoginScreen from './LoginScreen.svelte';

const savedMediaDevices = Object.getOwnPropertyDescriptor(navigator, 'mediaDevices');

beforeEach(() => {
	device.isMobile = true; // the phone's wizard
	app.authState = 'onboarding';
	mockTauriCommand('check_auth_state', () => ({
		needs_onboarding: true,
		needs_login: false,
		crypto_enabled: true
	}));
	mockTauriCommand('onboarding_designation', () => 'Ikshal-TEST-Ω');
	mockTauriCommand('validate_password_policy', () => ({ valid: true, errors: [] }));
	mockTauriCommand('toggle_theme', () => 'dark');
	mockTauriCommand('complete_onboarding', () => new Promise(() => {}));
	mockTauriCommand('complete_onboarding_paired', () => new Promise(() => {}));
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

const PASSWORD = 'Correct!Horse9Battery';

async function type(el: HTMLElement, value: string) {
	await fireEvent.input(el, { target: { value } });
	await settle();
}

function expectAttrs(fields: HTMLInputElement[], attrs: [string, string][], where: string) {
	for (const f of fields) {
		for (const [attr, value] of attrs) {
			expect(f.getAttribute(attr), `${where}, ${f.id || 'input'}: ${attr}`).toBe(value);
		}
	}
}

/** First-device flow, crypto on, to the password step. */
async function toPasswordStep(): Promise<Screen> {
	const screen = render(OnboardingWizard);
	await settle();
	await fireEvent.click(screen.getByText('First device'));
	await settle();
	await next(screen); // nickname
	await next(screen); // theme (the bubble step is skipped on mobile)
	await next(screen); // sample data
	await waitFor(() =>
		expect(screen.queryByRole('heading', { name: 'Set your password' })).not.toBeNull()
	);
	return screen;
}

/** Paired flow, with an offer pasted and a code typed, to its password step. */
async function toPairedPasswordStep(): Promise<Screen> {
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
	return screen;
}

/** First-device flow, through password, duress (empty) and canary (empty), to the keystroke step. */
async function toKeystrokeStep(): Promise<Screen> {
	const screen = await toPasswordStep();
	await type(screen.getByLabelText('Password'), PASSWORD);
	await type(screen.getByLabelText('Confirm password'), PASSWORD);
	await next(screen); // to duress
	await waitFor(() =>
		expect(screen.queryByRole('heading', { name: 'Duress password' })).not.toBeNull()
	);
	await next(screen); // duress left empty, to the canary step
	await waitFor(() =>
		expect(screen.queryByRole('heading', { name: /^(canary|safety) phrase$/i })).not.toBeNull()
	);
	await next(screen); // canary left empty, to keystroke enrollment
	await waitFor(() =>
		expect(screen.queryByRole('heading', { name: /keystroke enrollment/i })).not.toBeNull()
	);
	return screen;
}

describe('C9 — the main password may be saved by a password manager', () => {
	it('C9: the password step’s two inputs say new-password', async () => {
		const screen = await toPasswordStep();
		expectAttrs(
			[
				screen.getByLabelText('Password') as HTMLInputElement,
				screen.getByLabelText('Confirm password') as HTMLInputElement
			],
			[['autocomplete', 'new-password']],
			'password step'
		);
	});

	it('C9: the password step keeps its other attributes (the keyboard changes nothing typed)', async () => {
		const screen = await toPasswordStep();
		expectAttrs(
			[
				screen.getByLabelText('Password') as HTMLInputElement,
				screen.getByLabelText('Confirm password') as HTMLInputElement
			],
			[
				['autocapitalize', 'off'],
				['autocorrect', 'off'],
				['spellcheck', 'false']
			],
			'password step'
		);
	});
});

describe('C9 — the paired setup sets the same password, with the same attributes', () => {
	it('C9: the paired setup’s two password inputs say new-password, and the keyboard changes nothing', async () => {
		const screen = await toPairedPasswordStep();
		expectAttrs(
			[
				screen.getByLabelText('Password') as HTMLInputElement,
				screen.getByLabelText('Confirm password') as HTMLInputElement
			],
			[
				['autocomplete', 'new-password'],
				['autocapitalize', 'off'],
				['autocorrect', 'off'],
				['spellcheck', 'false']
			],
			'paired setup'
		);
	});
});

describe('C9 — the login lets a password manager fill the main password', () => {
	it('C9: the login’s password input says current-password, and the keyboard changes nothing', async () => {
		mockTauriCommand('access_recovery_available', () => false);
		mockTauriCommand('validate_password', () => 'primary');
		const screen = render(LoginScreen);
		await settle();
		const field = screen.getByPlaceholderText('Password') as HTMLInputElement;
		expectAttrs(
			[field],
			[
				['autocomplete', 'current-password'],
				['autocapitalize', 'off'],
				['autocorrect', 'off'],
				['spellcheck', 'false']
			],
			'login'
		);
	});
});

describe('C9 — the keystroke sample is typed, never filled', () => {
	it('C9: the keystroke sample input says autocomplete off', async () => {
		const screen = await toKeystrokeStep();
		const sample = screen.getByPlaceholderText('Type your password') as HTMLInputElement;
		expectAttrs([sample], [['autocomplete', 'off']], 'keystroke sample');
	});
});
