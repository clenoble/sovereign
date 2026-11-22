/** v0.0.13, lot C of the shell's accessibility (spec ch. 12, SQ10, « C »): two of the app's fields
 *  had a placeholder only, so a screen reader had no name for them. The phone's accessibility dump
 *  (`doc/plans/checks/phone-a11y-dump.py`, 2026-11-19) marked both « not accessibility friendly ».
 *  - The unlock screen's password field: a screen reader names it « Password », its placeholder kept
 *    (the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19).
 *  - The setup's field for the AI's nickname (« Name your AI »): named by a label, as the wizard's
 *    other fields are, its placeholder kept.
 *
 *  Written before the code, by another author than whoever implements it (WSL, lot C's test author):
 *  whoever implements makes these pass without editing this file. A test that looks wrong is
 *  reported, never worked around.
 *
 *  **Readings chosen** (reported with the tests):
 *  - A name counts when it comes from what a screen reader takes as a label: `aria-label`,
 *    `aria-labelledby`, or a `<label>` tied to the field. The placeholder does not count: the
 *    Android WebView did not give it as the node's name (the dump, 2026-11-19), and WCAG does not take
 *    a placeholder for a label (1.3.1, 4.1.2). Which of the three is used is the implementer's.
 *  - The nickname label's words are the implementer's: they must say what the field is, so they hold
 *    « name » (case aside), as « Nickname », « AI nickname » and « Name your AI » do.
 *  - « Placeholder kept »: the same placeholder text as today.
 *
 *  Same harness as `OnboardingWizard.autocomplete.svelte.test.ts`: the backend mocked at the Tauri
 *  `invoke` layer, the components driven through the DOM only. */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { device } from '$lib/stores/device.svelte';
import { app } from '$lib/stores/app.svelte';
import OnboardingWizard from './OnboardingWizard.svelte';
import LoginScreen from './LoginScreen.svelte';

const LOGIN_PLACEHOLDER = 'Password';
const NICKNAME_PLACEHOLDER = 'e.g. "Ike", "T-Nine", "B4"...';

beforeEach(() => {
	device.isMobile = true; // the phone, where the dump found both
	mockTauriCommand('check_auth_state', () => ({
		needs_onboarding: true,
		needs_login: false,
		crypto_enabled: true
	}));
	mockTauriCommand('access_recovery_available', () => false);
	mockTauriCommand('validate_password', () => 'primary');
	mockTauriCommand('onboarding_designation', () => 'Ikshal-TEST-Ω');
	mockTauriCommand('validate_password_policy', () => ({ valid: true, errors: [] }));
	mockTauriCommand('toggle_theme', () => 'dark');
	mockTauriCommand('complete_onboarding', () => new Promise(() => {}));
	mockTauriCommand('complete_onboarding_paired', () => new Promise(() => {}));
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

/** The name a screen reader takes from a label: `aria-label`, then `aria-labelledby`, then the
 *  `<label>` elements tied to the field. Never the placeholder. Whitespace collapsed. */
function labelName(el: HTMLElement): string {
	const clean = (s: string | null | undefined) => (s ?? '').replace(/\s+/g, ' ').trim();
	const aria = clean(el.getAttribute('aria-label'));
	if (aria) return aria;
	const by = el.getAttribute('aria-labelledby');
	if (by) {
		const text = clean(
			by
				.split(/\s+/)
				.map((id) => document.getElementById(id)?.textContent ?? '')
				.join(' ')
		);
		if (text) return text;
	}
	const labels = (el as HTMLInputElement).labels;
	return clean(Array.from(labels ?? []).map((l) => l.textContent ?? '').join(' '));
}

describe('lot C: the unlock screen password field has a name', () => {
	it('is named « Password » by a label, its placeholder kept', async () => {
		app.authState = 'login';
		const screen = render(LoginScreen);
		await settle();
		const input = screen.getByPlaceholderText(LOGIN_PLACEHOLDER) as HTMLInputElement;
		expect(input.type, 'precondition: the unlock field, hidden while typed').toBe('password');
		expect(labelName(input), 'the name a screen reader gives the field').toBe('Password');
		expect(input.getAttribute('placeholder'), 'the placeholder is kept').toBe(LOGIN_PLACEHOLDER);
	});

	it('keeps its name while the password is shown', async () => {
		app.authState = 'login';
		const screen = render(LoginScreen);
		await settle();
		await fireEvent.click(screen.getByRole('button', { name: 'Show password' }));
		await settle();
		const shown = screen.getByPlaceholderText(LOGIN_PLACEHOLDER) as HTMLInputElement;
		expect(shown.type, 'precondition: the password is shown').toBe('text');
		expect(labelName(shown), 'the same name, shown or hidden').toBe('Password');
	});
});

describe('lot C: the setup nickname field has a name', () => {
	it('is named by a label that says what it is, its placeholder kept', async () => {
		app.authState = 'onboarding';
		const screen = render(OnboardingWizard);
		await settle();
		await fireEvent.click(screen.getByText('First device'));
		await settle();
		await waitFor(() =>
			expect(screen.queryByRole('heading', { name: 'Name your AI' })).not.toBeNull()
		);
		const input = screen.getByPlaceholderText(NICKNAME_PLACEHOLDER) as HTMLInputElement;
		const name = labelName(input);
		expect(name, 'the field has a name a screen reader reads').not.toBe('');
		expect(name.toLowerCase(), `the name says what the field is: « ${name} »`).toContain('name');
		expect(input.getAttribute('placeholder'), 'the placeholder is kept').toBe(NICKNAME_PLACEHOLDER);
	});
});
