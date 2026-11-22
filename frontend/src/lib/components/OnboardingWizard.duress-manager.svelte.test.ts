/** v0.0.13 rc2 — the duress step says what a password manager means for a duress password (spec ch. 12,
 *  K7). K6 stays as built (« current-password » on the login, « new-password » on the setup); the duress
 *  step has one more sentence, the duress password field's description, so a screen reader that lands on
 *  the field says it. Céline, 2026-11-20: « c'est la responsabilité de l'utilisateur, on ne peut pas tout
 *  gérer à sa place ».
 *
 *  Written before the code, by another author than whoever implements it (WSL, as lot R3's test
 *  author): whoever implements makes these pass without editing this file. A test that looks wrong is
 *  reported, never worked around.
 *
 *  Same harness as `OnboardingWizard.keystroke-name.svelte.test.ts`: the backend is mocked at the Tauri
 *  `invoke` layer, and the component is driven through the DOM only. */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { device } from '$lib/stores/device.svelte';
import { app } from '$lib/stores/app.svelte';
import OnboardingWizard from './OnboardingWizard.svelte';

const SENTENCE =
	'A password manager on this device may offer your main password when you unlock, and may offer to ' +
	'save this one. If you rely on a duress password, keep both out of any password manager.';

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

/** The text with its runs of white space made one, as a reader hears it. */
function said(text: string | null | undefined): string {
	return (text ?? '').replace(/\s+/g, ' ').trim();
}

/** First-device flow, crypto on, to the password step. On the desktop the bubble-style step comes
 *  after the nickname; on the phone it is skipped. */
async function toPasswordStep(): Promise<Screen> {
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
	return screen;
}

async function toDuressStep(): Promise<Screen> {
	const screen = await toPasswordStep();
	await type(screen.getByLabelText('Password'), PASSWORD);
	await type(screen.getByLabelText('Confirm password'), PASSWORD);
	await next(screen);
	await waitFor(() =>
		expect(screen.queryByRole('heading', { name: 'Duress password' })).not.toBeNull()
	);
	return screen;
}

for (const [where, mobile] of [
	['the phone', true],
	['the desktop app', false]
] as const) {
	describe(`the duress step says what a password manager means for it, on ${where}`, () => {
		beforeEach(() => {
			device.isMobile = mobile;
		});

		it('shows the sentence on the duress step, fields empty', async () => {
			const screen = await toDuressStep();
			const shown = Array.from(screen.container.querySelectorAll('p')).map((p) => said(p.textContent));
			expect(shown).toContain(SENTENCE);
		});

		it('is the duress password field’s accessible description', async () => {
			const screen = await toDuressStep();
			const field = screen.getByLabelText('Duress password', { selector: 'input' });
			expect(field).toHaveAccessibleDescription(expect.stringContaining(SENTENCE));
		});

		it('still shows it once the fields are filled', async () => {
			const screen = await toDuressStep();
			await type(screen.getByLabelText('Duress password', { selector: 'input' }), 'Other!Pass7word');
			await type(screen.getByLabelText('Confirm', { selector: 'input' }), 'Other!Pass7word');
			const shown = Array.from(screen.container.querySelectorAll('p')).map((p) => said(p.textContent));
			expect(shown).toContain(SENTENCE);
		});

		it('is not shown on the password step', async () => {
			const screen = await toPasswordStep();
			expect(said(screen.container.textContent)).not.toContain(SENTENCE);
		});
	});
}
