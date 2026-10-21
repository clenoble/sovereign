/** rc2 — while the account is being created, the wizard shows that it is working and
 *  cannot start (or leave) a second creation (spec ch. 12, *Onboarding and the auth gate*,
 *  "While the account is being created", v2.0.46): "a spinner and a line saying what is
 *  happening — and every button that could start or leave account creation ("Get started",
 *  "Skip", "Back") is disabled until it ends; if it fails, the error is shown on the screen,
 *  not only in a log." On rc1 a second tap on "Get Started", during the ~10 s of key
 *  derivation on a phone, started a second creation that replaced auth.store.
 *
 *  Written before the code, by another author: whoever implements rc2 makes these pass
 *  without editing this file. The backend is mocked at the Tauri `invoke` layer
 *  (`$lib/test/tauri`, as `LoginScreen.svelte.test.ts` does); `complete_onboarding` and
 *  `complete_onboarding_paired` return promises the test settles. Selectors are by role and
 *  text. "Working" is an element with role `status` (WCAG 4.1.3) or the text "Creating your
 *  account"; for the paired flow the wizard's existing line ("Connecting to …") also counts. */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { device } from '$lib/stores/device.svelte';
import OnboardingWizard from './OnboardingWizard.svelte';

type Deferred = { promise: Promise<void>; resolve: () => void; reject: (e: unknown) => void };
function deferred(): Deferred {
	let resolve!: () => void;
	let reject!: (e: unknown) => void;
	const promise = new Promise<void>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

let completeCalls: unknown[];
let pairedCalls: unknown[];
let pending: Deferred;
const PASSWORD = 'Correct!Horse9Battery';

const savedMediaDevices = Object.getOwnPropertyDescriptor(navigator, 'mediaDevices');

beforeEach(() => {
	completeCalls = [];
	pairedCalls = [];
	pending = deferred();
	// The phone's wizard (spec ch. 12: "the mobile wizard").
	device.isMobile = true;
	mockTauriCommand('check_auth_state', () => ({
		needs_onboarding: true,
		needs_login: false,
		crypto_enabled: true
	}));
	mockTauriCommand('onboarding_designation', () => 'Ikshal-TEST-Ω');
	mockTauriCommand('validate_password_policy', () => ({ valid: true, errors: [] }));
	mockTauriCommand('toggle_theme', () => 'dark');
	mockTauriCommand('complete_onboarding', (args) => {
		completeCalls.push(args);
		return pending.promise;
	});
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
});

afterEach(() => {
	device.isMobile = false;
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

type Screen = ReturnType<typeof render>;

function button(screen: Screen, name: string | RegExp): HTMLButtonElement {
	return screen.getByRole('button', { name }) as HTMLButtonElement;
}

async function next(screen: Screen) {
	await waitFor(() => expect(button(screen, 'Next')).toBeEnabled());
	await fireEvent.click(button(screen, 'Next'));
	await settle();
}

async function typePasswordPair(screen: Screen) {
	await fireEvent.input(screen.getByPlaceholderText('Enter password'), {
		target: { value: PASSWORD }
	});
	await fireEvent.input(screen.getByPlaceholderText('Confirm password'), {
		target: { value: PASSWORD }
	});
	await settle();
}

/** First-device flow, crypto on, to its last step (keystroke enrollment), where both
 *  "Get Started" and "Skip this step" start account creation. */
async function toFinalStep(): Promise<Screen> {
	const screen = render(OnboardingWizard);
	await settle();
	await fireEvent.click(screen.getByText('First device'));
	await settle();
	await next(screen); // nickname
	await next(screen); // theme (the bubble step is skipped on mobile)
	await next(screen); // sample data
	await typePasswordPair(screen); // password
	await next(screen);
	await next(screen); // duress (optional)
	await next(screen); // canary (optional)
	await waitFor(() => expect(button(screen, 'Get Started')).toBeInTheDocument());
	return screen;
}

function showsWork(screen: Screen, extra?: RegExp): boolean {
	return (
		screen.queryAllByRole('status').length > 0 ||
		screen.queryByText(/Creating your account/i) !== null ||
		(extra !== undefined && screen.queryByText(extra) !== null)
	);
}

function expectLocked(screen: Screen) {
	expect(button(screen, 'Get Started'), '"Get Started" while creating').toBeDisabled();
	expect(button(screen, 'Back'), '"Back" while creating').toBeDisabled();
	for (const skip of screen.queryAllByRole('button', { name: 'Skip this step' })) {
		expect(skip, '"Skip this step" while creating').toBeDisabled();
	}
}

describe('OnboardingWizard while the account is being created (first device)', () => {
	it('starts one creation however many times "Get Started" is pressed', async () => {
		const screen = await toFinalStep();
		await fireEvent.click(button(screen, 'Get Started'));
		await settle();
		await fireEvent.click(button(screen, 'Get Started'));
		await settle();

		expect(completeCalls).toHaveLength(1);
		pending.resolve();
		await settle();
	});

	it('disables "Get Started", "Back" and "Skip this step" and shows that it is working', async () => {
		const screen = await toFinalStep();
		expect(screen.queryAllByRole('button', { name: 'Skip this step' }).length).toBeGreaterThan(0);
		await fireEvent.click(button(screen, 'Get Started'));
		await settle();

		expect(completeCalls).toHaveLength(1);
		expectLocked(screen);
		expect(
			showsWork(screen),
			'a progress element (role "status" or "Creating your account") while creating'
		).toBe(true);

		pending.resolve();
		await settle();
	});

	it('does not start a second creation from "Skip this step" on the last step', async () => {
		const screen = await toFinalStep();
		await fireEvent.click(button(screen, 'Skip this step'));
		await settle();
		expect(completeCalls).toHaveLength(1);

		for (const skip of screen.queryAllByRole('button', { name: 'Skip this step' })) {
			await fireEvent.click(skip);
		}
		await fireEvent.click(button(screen, 'Get Started'));
		await settle();

		expect(completeCalls).toHaveLength(1);
		expectLocked(screen);
		pending.resolve();
		await settle();
	});

	it('shows the error on the screen when creation fails', async () => {
		const screen = await toFinalStep();
		await fireEvent.click(button(screen, 'Get Started'));
		await settle();
		// What the backend answers to a second request (spec ch. 07, Auth store).
		pending.reject('Setup is already in progress');
		await settle();

		await waitFor(() =>
			expect(screen.getByText(/Setup is already in progress/)).toBeInTheDocument()
		);
	});

	// Settled by the coordinator (2026-10-19, ambiguity #1 of the first report): after a
	// failed creation the buttons are enabled again, otherwise the user is stuck.
	it('gives the buttons back after a failed creation, with the error shown', async () => {
		const screen = await toFinalStep();
		await fireEvent.click(button(screen, 'Get Started'));
		await settle();
		expectLocked(screen);

		pending.reject('Already onboarded — log in instead');
		await settle();

		await waitFor(() =>
			expect(screen.getByText(/Already onboarded — log in instead/)).toBeInTheDocument()
		);
		expect(button(screen, 'Get Started'), '"Get Started" after a failure').toBeEnabled();
		expect(button(screen, 'Back'), '"Back" after a failure').toBeEnabled();
		for (const skip of screen.queryAllByRole('button', { name: 'Skip this step' })) {
			expect(skip, '"Skip this step" after a failure').toBeEnabled();
		}
	});
});

describe('OnboardingWizard while the account is being created (paired device)', () => {
	async function toPairedPassword(): Promise<Screen> {
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
		return screen;
	}

	it('starts one pairing, disables "Get Started" and "Back", and shows that it is working', async () => {
		const screen = await toPairedPassword();
		await fireEvent.click(button(screen, 'Get Started'));
		await settle();
		await fireEvent.click(button(screen, 'Get Started'));
		await settle();

		expect(pairedCalls).toHaveLength(1);
		expectLocked(screen);
		expect(showsWork(screen, /Connecting to/), 'a progress line while pairing').toBe(true);

		pending.resolve();
		await settle();
	});
});
