/** v0.0.13 rc2 — the keystroke enrolment's field is named for a screen reader (spec ch. 12, lot C of
 *  « Every control is reached from the keyboard and named »): as the wizard's other fields are, by a
 *  label, « Password sample », its placeholder kept. Found by the phone's accessibility dump and the
 *  page's markup on rc1 (2026-11-19): the field had a placeholder only. Céline, 2026-11-20, on the page
 *  of rc1's findings: « oui, à faire ».
 *
 *  Written before the code, by another author than whoever implements it (WSL, as lot R3's test
 *  author): whoever implements makes these pass without editing this file. A test that looks wrong is
 *  reported, never worked around.
 *
 *  Same harness as `OnboardingWizard.autocomplete.svelte.test.ts`: the backend is mocked at the Tauri
 *  `invoke` layer, and the component is driven through the DOM only (fields by label, buttons by role
 *  and text). */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { device } from '$lib/stores/device.svelte';
import { app } from '$lib/stores/app.svelte';
import OnboardingWizard from './OnboardingWizard.svelte';

const NAME = 'Password sample';
const PLACEHOLDER = 'Type your password';

beforeEach(() => {
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
});

afterEach(() => {
	device.isMobile = false;
	app.authState = 'checking';
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

/** First-device flow, crypto on, through password, duress (empty) and canary (empty), to the
 *  keystroke step. On the desktop the bubble-style step comes after the nickname; on the phone it
 *  is skipped. */
async function toKeystrokeStep(): Promise<Screen> {
	const screen = render(OnboardingWizard);
	await settle();
	await fireEvent.click(screen.getByText('First device'));
	await settle();
	await next(screen); // nickname
	if (!device.isMobile) {
		await next(screen); // bubble style
	}
	await next(screen); // theme
	await next(screen); // sample data
	await waitFor(() =>
		expect(screen.queryByRole('heading', { name: 'Set your password' })).not.toBeNull()
	);
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

for (const [where, mobile] of [
	['the phone', true],
	['the desktop app', false]
] as const) {
	describe(`the keystroke sample's field is named, on ${where}`, () => {
		beforeEach(() => {
			device.isMobile = mobile;
		});

		it(`is reached by its name, « ${NAME} »`, async () => {
			const screen = await toKeystrokeStep();
			const byName = screen.getByLabelText(NAME, { selector: 'input' }) as HTMLInputElement;
			expect(byName.type).toBe('password');
		});

		it('is the one field of the step, and keeps its placeholder', async () => {
			const screen = await toKeystrokeStep();
			const byName = screen.getByLabelText(NAME, { selector: 'input' });
			const byPlaceholder = screen.getByPlaceholderText(PLACEHOLDER);
			expect(byName).toBe(byPlaceholder);
		});

		it('keeps what it already said: never filled by a password manager (K6)', async () => {
			const screen = await toKeystrokeStep();
			const field = screen.getByLabelText(NAME, { selector: 'input' });
			expect(field.getAttribute('autocomplete')).toBe('off');
		});

		it('is still named after a sample is submitted (the field is drawn again)', async () => {
			const screen = await toKeystrokeStep();
			const field = screen.getByLabelText(NAME, { selector: 'input' });
			await type(field, PASSWORD);
			await fireEvent.click(button(screen, 'Submit sample'));
			await settle();
			await waitFor(() => expect(screen.getByText(/Sample 2 of 5/)).toBeTruthy());
			expect(screen.getByLabelText(NAME, { selector: 'input' })).toBeTruthy();
		});
	});
}
