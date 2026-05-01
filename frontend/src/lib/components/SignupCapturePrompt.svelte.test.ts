import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import SignupCapturePrompt from './SignupCapturePrompt.svelte';
import { piiState } from '$lib/stores/pii.svelte';
import type { BrowserFormExtraction, SignupCaptureResult } from '$lib/api/commands';

function makeField(kind: string, selector: string, value = '', label = ''): BrowserFormExtraction['fields'][number] {
	return { kind, selector, value, placeholder: '', label };
}

function makeExtraction(url: string, fields: BrowserFormExtraction['fields']): BrowserFormExtraction {
	return { url, fields };
}

beforeEach(() => {
	piiState.entities = [];
	piiState.records = [];
});

describe('SignupCapturePrompt — render gating', () => {
	it('renders nothing when open=false', () => {
		const { container } = render(SignupCapturePrompt, {
			props: {
				open: false,
				extraction: makeExtraction('https://example.com', [makeField('password', '#pw')]),
				onClose: () => {}
			}
		});
		expect(container.querySelector('.capture-dialog')).toBeNull();
	});

	it('renders nothing when extraction is null', () => {
		const { container } = render(SignupCapturePrompt, {
			props: { open: true, extraction: null, onClose: () => {} }
		});
		expect(container.querySelector('.capture-dialog')).toBeNull();
	});
});

describe('SignupCapturePrompt — initial row population', () => {
	it('renders one row per extraction field', () => {
		const { container } = render(SignupCapturePrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://signup.example.com', [
					makeField('email', '#email', 'me@x.com'),
					makeField('password', '#pw'),
					makeField('first_name', '#first', 'Alex')
				]),
				onClose: () => {}
			}
		});
		expect(container.querySelectorAll('.field-row').length).toBe(3);
	});

	it('auto-includes password fields even when their value is empty', () => {
		const { container } = render(SignupCapturePrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://signup.example.com', [
					makeField('password', '#pw') // value=''
				]),
				onClose: () => {}
			}
		});
		const checkbox = container.querySelector('input[type="checkbox"]') as HTMLInputElement;
		expect(checkbox.checked).toBe(true);
	});

	it('auto-includes non-password fields ONLY when they have a non-empty value', () => {
		const { container } = render(SignupCapturePrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://signup.example.com', [
					makeField('email', '#email', 'me@x.com'), // included
					makeField('first_name', '#first') // empty → not included
				]),
				onClose: () => {}
			}
		});
		const checkboxes = container.querySelectorAll(
			'input[type="checkbox"]'
		) as NodeListOf<HTMLInputElement>;
		expect(checkboxes[0].checked).toBe(true);
		expect(checkboxes[1].checked).toBe(false);
	});
});

describe('SignupCapturePrompt — generate password', () => {
	it('clicking Generate calls generate_password and updates the row value', async () => {
		mockTauriCommand('generate_password', () => 'GeneratedPa$$w0rd!42');

		const { container } = render(SignupCapturePrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://signup.example.com', [
					makeField('password', '#pw')
				]),
				onClose: () => {}
			}
		});

		const generateBtn = Array.from(container.querySelectorAll('button')).find((b) =>
			b.textContent?.includes('Generate')
		) as HTMLButtonElement;
		await fireEvent.click(generateBtn);
		await tick();

		const valueInput = container.querySelector('input[type="password"]') as HTMLInputElement;
		expect(valueInput.value).toBe('GeneratedPa$$w0rd!42');
	});
});

describe('SignupCapturePrompt — submit', () => {
	it('shows error when no rows are included', async () => {
		const { container } = render(SignupCapturePrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://signup.example.com', [
					// non-password, empty → not auto-included
					makeField('first_name', '#first')
				]),
				onClose: () => {}
			}
		});
		const form = container.querySelector('form') as HTMLFormElement;
		await fireEvent.submit(form);
		await tick();
		expect(container.querySelector('.error')?.textContent).toContain('No fields selected');
	});

	it('commits only the included rows with their kind/label/value', async () => {
		const commit = vi.fn(
			(args: { input: { url: string; fields: Array<{ kind: string; value: string }> } }) => {
				expect(args.input.url).toBe('https://signup.example.com/');
				// Only password and email should be sent — first_name was empty and not auto-included.
				const kinds = args.input.fields.map((f) => f.kind).sort();
				expect(kinds).toEqual(['email', 'password']);
				return {
					entity_id: 'e:new',
					record_ids: ['p:1', 'p:2'],
					share_record_count: 2,
					entity_created: true
				} satisfies SignupCaptureResult;
			}
		);
		mockTauriCommand('commit_signup_capture', commit);
		mockTauriCommand('list_pii_entities', () => []);
		mockTauriCommand('list_pii_records', () => []);

		const { container } = render(SignupCapturePrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://signup.example.com/', [
					makeField('email', '#email', 'me@x.com'),
					makeField('password', '#pw'),
					makeField('first_name', '#first') // empty — not included
				]),
				onClose: () => {}
			}
		});

		// Type a password value so the password row passes the value-length filter
		const valueInputs = container.querySelectorAll('input.value-input');
		const pwInput = valueInputs[1] as HTMLInputElement;
		await fireEvent.input(pwInput, { target: { value: 'secret123' } });

		const form = container.querySelector('form') as HTMLFormElement;
		await fireEvent.submit(form);
		await tick();
		await tick();
		await tick();

		expect(commit).toHaveBeenCalledTimes(1);
		expect(container.querySelector('.success')?.textContent).toContain('Saved');
		expect(container.querySelector('.success')?.textContent).toContain('new entity created');
	});

	it('shows the error and stays open when commit_signup_capture rejects', async () => {
		mockTauriCommand('commit_signup_capture', () => {
			throw new Error('entity collision');
		});

		const { container } = render(SignupCapturePrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://signup.example.com', [
					makeField('email', '#email', 'me@x.com')
				]),
				onClose: () => {}
			}
		});

		const form = container.querySelector('form') as HTMLFormElement;
		await fireEvent.submit(form);
		await tick();
		await tick();

		expect(container.querySelector('.error')?.textContent).toContain('entity collision');
		expect(container.querySelector('.success')).toBeNull();
	});
});
