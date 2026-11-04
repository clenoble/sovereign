/** v0.0.12 — the app wizard's duress and canary steps get the password step's "show" toggle
 *  (spec ch. 12, K3–K5, v2.0.69; contract `doc/plans/V0012-phone-show.md`).
 *
 *  Written from the contract only, before the code, by another author (PANDA2): whoever
 *  implements makes these pass without editing this file. A test that looks wrong is reported,
 *  never worked around.
 *
 *  Same harness as `OnboardingWizard.p5.svelte.test.ts`: the backend is mocked at the Tauri
 *  `invoke` layer, and the component is driven through the DOM only (fields by label, the
 *  toggle by its label, buttons by role and text). */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { device } from '$lib/stores/device.svelte';
import { app } from '$lib/stores/app.svelte';
import OnboardingWizard from './OnboardingWizard.svelte';

type Sent = Record<string, unknown>;

let completeCalls: { data: Sent }[];
let resolveCreate: (v?: unknown) => void;

const PASSWORD = 'Correct!Horse9Battery';
const DURESS = 'decoy!garden7lantern'; // lower case first: K5 is about what the keyboard would change
const PHRASE = 'the cat sleeps on warm roofs';

beforeEach(() => {
	completeCalls = [];
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
	mockTauriCommand('complete_onboarding', (args) => {
		completeCalls.push(args as { data: Sent });
		return new Promise((res) => {
			resolveCreate = res;
		});
	});
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

async function type(el: HTMLElement, value: string) {
	await fireEvent.input(el, { target: { value } });
	await settle();
}

async function click(el: HTMLElement) {
	await fireEvent.click(el);
	await settle();
}

const onDuressStep = (s: Screen) => s.queryByRole('heading', { name: 'Duress password' }) !== null;
const onPhraseStep = (s: Screen) =>
	s.queryByRole('heading', { name: /^(canary|safety) phrase$/i }) !== null;

const duressFields = (s: Screen) => [
	s.getByLabelText('Duress password') as HTMLInputElement,
	s.getByLabelText('Confirm') as HTMLInputElement
];
const phraseFields = (s: Screen) => [
	s.getByLabelText(/^(canary|safety) phrase$/i) as HTMLInputElement,
	s.getByLabelText(/^confirm phrase$/i) as HTMLInputElement
];
const toggle = (s: Screen, name: string) => {
	const el = s.getByLabelText(name) as HTMLInputElement;
	expect(el.type, `"${name}" is a checkbox`).toBe('checkbox');
	return el;
};

/** First-device flow, crypto on, to the duress step. */
async function toDuressStep(): Promise<Screen> {
	const screen = render(OnboardingWizard);
	await settle();
	await fireEvent.click(screen.getByText('First device'));
	await settle();
	await next(screen); // nickname
	await next(screen); // theme (the bubble step is skipped on mobile)
	await next(screen); // sample data
	await type(screen.getByLabelText('Password'), PASSWORD);
	await type(screen.getByLabelText('Confirm password'), PASSWORD);
	await next(screen);
	await waitFor(() => expect(onDuressStep(screen)).toBe(true));
	return screen;
}

/** From the duress step, with nothing set there, to the canary step. */
async function toPhraseStep(): Promise<Screen> {
	const screen = await toDuressStep();
	await next(screen);
	await waitFor(() => expect(onPhraseStep(screen)).toBe(true));
	return screen;
}

/** From the duress or canary step, onward to Create ("Get Started"). */
async function createFromHere(screen: Screen) {
	for (let i = 0; i < 3 && screen.queryByRole('button', { name: 'Get Started' }) === null; i++) {
		await next(screen);
	}
	await waitFor(() => expect(button(screen, 'Get Started')).toBeEnabled());
	await fireEvent.click(button(screen, 'Get Started'));
	await settle();
}

const types = (fields: HTMLInputElement[]) => fields.map((f) => f.type);

// ---------------------------------------------------------------------------
// K3 — the duress step
// ---------------------------------------------------------------------------
describe('K3 — the duress step has the password step’s "show" toggle', () => {
	it('K3: a "Show password" checkbox, unchecked, and both duress inputs masked', async () => {
		const screen = await toDuressStep();
		expect(toggle(screen, 'Show password').checked).toBe(false);
		expect(types(duressFields(screen))).toEqual(['password', 'password']);
	});

	it('K3: checked, both duress inputs are text; unchecked again, both are masked', async () => {
		const screen = await toDuressStep();
		await click(toggle(screen, 'Show password'));
		expect(types(duressFields(screen))).toEqual(['text', 'text']);
		await click(toggle(screen, 'Show password'));
		expect(types(duressFields(screen))).toEqual(['password', 'password']);
	});
});

// ---------------------------------------------------------------------------
// K3 — the canary step
// ---------------------------------------------------------------------------
describe('K3 — the canary step has the same toggle, labelled for the phrase', () => {
	it('K3: a "Show phrase" checkbox, unchecked, and both canary inputs masked', async () => {
		const screen = await toPhraseStep();
		expect(toggle(screen, 'Show phrase').checked).toBe(false);
		expect(types(phraseFields(screen))).toEqual(['password', 'password']);
		expect(screen.queryByLabelText('Show password'), 'no "Show password" on this step').toBeNull();
	});

	it('K3: checked, both canary inputs are text; unchecked again, both are masked', async () => {
		const screen = await toPhraseStep();
		await click(toggle(screen, 'Show phrase'));
		expect(types(phraseFields(screen))).toEqual(['text', 'text']);
		await click(toggle(screen, 'Show phrase'));
		expect(types(phraseFields(screen))).toEqual(['password', 'password']);
	});
});

// ---------------------------------------------------------------------------
// K3 — one state for the whole wizard
// ---------------------------------------------------------------------------
describe('K3 — one "show" state for the whole wizard', () => {
	it('K3: checked on the duress step, the canary step opens shown, its box checked', async () => {
		const screen = await toDuressStep();
		await click(toggle(screen, 'Show password'));
		await next(screen); // both duress fields empty: Next moves on
		await waitFor(() => expect(onPhraseStep(screen)).toBe(true));
		expect(toggle(screen, 'Show phrase').checked).toBe(true);
		expect(types(phraseFields(screen))).toEqual(['text', 'text']);
	});

	it('K3: checked on the password step, the duress step opens shown, its box checked', async () => {
		const screen = render(OnboardingWizard);
		await settle();
		await fireEvent.click(screen.getByText('First device'));
		await settle();
		await next(screen);
		await next(screen);
		await next(screen);
		await click(toggle(screen, 'Show password'));
		await type(screen.getByLabelText('Password'), PASSWORD);
		await type(screen.getByLabelText('Confirm password'), PASSWORD);
		await next(screen);
		await waitFor(() => expect(onDuressStep(screen)).toBe(true));
		expect(toggle(screen, 'Show password').checked).toBe(true);
		expect(types(duressFields(screen))).toEqual(['text', 'text']);
	});
});

// ---------------------------------------------------------------------------
// K5 — the password step's attributes, on the four secret inputs
// ---------------------------------------------------------------------------
const K5: [string, string][] = [
	['autocapitalize', 'off'],
	['autocorrect', 'off'],
	['autocomplete', 'off'],
	['spellcheck', 'false']
];

function expectK5(fields: HTMLInputElement[], step: string) {
	for (const f of fields) {
		for (const [attr, value] of K5) {
			expect(f.getAttribute(attr), `${step}, ${f.id || 'input'}: ${attr}`).toBe(value);
		}
	}
}

describe('K5 — the duress and canary inputs carry the password step’s attributes', () => {
	it('K5: both duress inputs, masked and shown', async () => {
		const screen = await toDuressStep();
		expectK5(duressFields(screen), 'duress, masked');
		await click(toggle(screen, 'Show password'));
		expectK5(duressFields(screen), 'duress, shown');
	});

	it('K5: both canary inputs, masked and shown', async () => {
		const screen = await toPhraseStep();
		expectK5(phraseFields(screen), 'canary, masked');
		await click(toggle(screen, 'Show phrase'));
		expectK5(phraseFields(screen), 'canary, shown');
	});

	it('K5: control — the password step’s own inputs carry them (the reference K5 copies)', async () => {
		const screen = render(OnboardingWizard);
		await settle();
		await fireEvent.click(screen.getByText('First device'));
		await settle();
		await next(screen);
		await next(screen);
		await next(screen);
		expectK5(
			[
				screen.getByLabelText('Password') as HTMLInputElement,
				screen.getByLabelText('Confirm password') as HTMLInputElement
			],
			'password step'
		);
	});
});

// ---------------------------------------------------------------------------
// K4 — what is sent is the same with "show" on
// ---------------------------------------------------------------------------
describe('K4 — "show" on changes nothing in what is sent', () => {
	it('K4: duress and phrase typed with "show" on are sent as typed', async () => {
		const screen = await toDuressStep();
		await click(toggle(screen, 'Show password'));
		const [d, dc] = duressFields(screen);
		await type(d, DURESS);
		await type(dc, DURESS);
		await next(screen);
		await waitFor(() => expect(onPhraseStep(screen)).toBe(true));
		const [p, pc] = phraseFields(screen);
		await type(p, PHRASE);
		await type(pc, PHRASE);
		await createFromHere(screen);
		expect(completeCalls, 'one complete_onboarding call').toHaveLength(1);
		const data = completeCalls[0].data;
		expect(data.duress_password).toBe(DURESS);
		expect(data.duress_confirm).toBe(DURESS);
		expect(data.canary_phrase).toBe(PHRASE);
		expect(data.canary_confirm).toBe(PHRASE);
		resolveCreate();
		await settle();
	});
});
