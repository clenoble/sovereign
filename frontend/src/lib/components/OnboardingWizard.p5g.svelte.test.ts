/** v0.0.12 P5h — the paired flow after an existing account (contract
 *  `doc/plans/V0012-p5-wizard.md`).
 *
 *  Written from the contract only, before the code, by another author: whoever implements
 *  P5h makes these pass without editing this file. Same harness as
 *  `OnboardingWizard.p5.svelte.test.ts`: the backend is mocked at the Tauri `invoke` layer;
 *  `complete_onboarding_paired` returns a promise the test settles; the wizard is driven
 *  through the DOM only. (P5g's tests are in `LoginScreen.p5g.svelte.test.ts`.) */
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

let pairedCalls: unknown[];
let pending: Deferred;

const PASSWORD = 'Correct!Horse9Battery';
const savedMediaDevices = Object.getOwnPropertyDescriptor(navigator, 'mediaDevices');

beforeEach(() => {
	pairedCalls = [];
	pending = deferred();
	device.isMobile = true; // the phone's wizard
	app.authState = 'onboarding';
	mockTauriCommand('check_auth_state', () => ({
		needs_onboarding: true,
		needs_login: false,
		crypto_enabled: true,
		setup_incomplete: false
	}));
	mockTauriCommand('onboarding_designation', () => 'Ikshal-TEST-Ω');
	mockTauriCommand('validate_password_policy', () => ({ valid: true, errors: [] }));
	mockTauriCommand('toggle_theme', () => 'dark');
	mockTauriCommand('complete_onboarding_paired', (args) => {
		pairedCalls.push(args);
		return pending.promise;
	});
	mockTauriCommand('consume_pair_qr_preview', () => ({
		source_peer_id: '12D3KooWTest',
		source_device_name: 'Desk',
		issued_at: 0,
		expires_at: 4102444800
	}));
	// A camera that never answers, so the scanner offers its manual-paste button.
	Object.defineProperty(navigator, 'mediaDevices', {
		configurable: true,
		value: { getUserMedia: () => new Promise(() => {}) }
	});
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

async function type(el: HTMLElement, value: string) {
	await fireEvent.input(el, { target: { value } });
	await settle();
}

/** The paired flow, from the first screen to Create ("Get Started"), pressed. */
async function createPaired(): Promise<Screen> {
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
	await type(screen.getByPlaceholderText('XXXXX-XXXXX'), 'ABCDE-FGHJK');
	await waitFor(() => expect(button(screen, 'Next')).toBeEnabled());
	await fireEvent.click(button(screen, 'Next'));
	await settle();
	await type(screen.getByLabelText('Password'), PASSWORD);
	await type(screen.getByLabelText('Confirm password'), PASSWORD);
	await waitFor(() => expect(button(screen, 'Get Started')).toBeEnabled());
	await fireEvent.click(button(screen, 'Get Started'));
	await settle();
	expect(pairedCalls, 'one complete_onboarding_paired call').toHaveLength(1);
	return screen;
}

describe('P5h — the paired flow after an existing account', () => {
	it('P5h: "Already onboarded" from completeOnboardingPaired sets the app to the login screen', async () => {
		await createPaired();
		pending.reject('Already onboarded — log in instead');
		await settle();
		await waitFor(() => expect(app.authState).toBe('login'));
	});

	it('P5h: a message that only contains "Already onboarded" is matched too', async () => {
		await createPaired();
		pending.reject(new Error('claim refused: Already onboarded'));
		await settle();
		await waitFor(() => expect(app.authState).toBe('login'));
	});

	it('P5h: the former "onboarding already complete on this device" refusal is not matched', async () => {
		await createPaired();
		pending.reject('onboarding already complete on this device');
		await settle();
		expect(app.authState).toBe('onboarding');
	});

	it('P5h: a paired result { incomplete: true } goes to the login screen', async () => {
		await createPaired();
		pending.resolve({ incomplete: true });
		await settle();
		await waitFor(() => expect(app.authState).toBe('login'));
	});

	it('P5h: any paired success goes to the login screen', async () => {
		await createPaired();
		pending.resolve(undefined);
		await settle();
		await waitFor(() => expect(app.authState).toBe('login'));
	});
});
