import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import AutofillPrompt from './AutofillPrompt.svelte';
import { piiState } from '$lib/stores/pii.svelte';
import type { BrowserFormExtraction, PiiEntity, PiiRecord } from '$lib/api/commands';

function makeEntity(id: string, name: string, domains: string[]): PiiEntity {
	return {
		id,
		name,
		kind: 'service',
		domains,
		contact_ids: [],
		notes: '',
		is_owned: false,
		created_at: '2026-04-01T00:00:00Z',
		modified_at: '2026-04-01T00:00:00Z'
	};
}

function makeVaultRecord(id: string, entityId: string, label: string): PiiRecord {
	return {
		id,
		kind: 'password',
		label,
		entity_id: entityId,
		stored_secret: true,
		confidence: 1.0,
		review_state: 'confirmed',
		discovered_at: '2026-04-15T00:00:00Z',
		last_revealed_at: null,
		use_count: 0,
		sources: []
	};
}

function makeExtraction(url: string, fields: Array<{ kind: string; selector: string }>): BrowserFormExtraction {
	return {
		url,
		fields: fields.map((f) => ({
			kind: f.kind,
			selector: f.selector,
			value: '',
			placeholder: '',
			label: ''
		}))
	};
}

beforeEach(() => {
	piiState.entities = [];
	piiState.records = [];
});

describe('AutofillPrompt — render gating', () => {
	it('renders nothing when open=false', () => {
		const { container } = render(AutofillPrompt, {
			props: {
				open: false,
				extraction: makeExtraction('https://github.com/login', [
					{ kind: 'password', selector: '#pw' }
				]),
				onClose: () => {}
			}
		});
		expect(container.querySelector('.autofill-dialog')).toBeNull();
	});

	it('renders nothing when extraction is null', () => {
		const { container } = render(AutofillPrompt, {
			props: { open: true, extraction: null, onClose: () => {} }
		});
		expect(container.querySelector('.autofill-dialog')).toBeNull();
	});
});

describe('AutofillPrompt — domain matching warnings', () => {
	it('shows the "no password field" warning when extraction has no password input', () => {
		const { container } = render(AutofillPrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://example.com', [
					{ kind: 'email', selector: '#email' }
				]),
				onClose: () => {}
			}
		});
		expect(container.querySelector('.warning')?.textContent).toContain(
			'No password field detected'
		);
	});

	it('shows the "no entity" warning when host does not match any entity domain', () => {
		piiState.entities = [makeEntity('e:other', 'Other', ['somewhere-else.com'])];
		const { container } = render(AutofillPrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://github.com/login', [
					{ kind: 'password', selector: '#pw' }
				]),
				onClose: () => {}
			}
		});
		expect(container.querySelector('.warning')?.textContent).toContain('No entity');
	});

	it('shows the "no vault entries" warning when entity matches but has no stored secrets', () => {
		piiState.entities = [makeEntity('e:gh', 'GitHub', ['github.com'])];
		piiState.records = []; // matched entity but no vault entries
		const { container } = render(AutofillPrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://github.com/login', [
					{ kind: 'password', selector: '#pw' }
				]),
				onClose: () => {}
			}
		});
		expect(container.querySelector('.warning')?.textContent).toContain('No vault entries');
	});
});

describe('AutofillPrompt — domain matching logic', () => {
	it('matches the host exactly against entity.domains', () => {
		piiState.entities = [makeEntity('e:gh', 'GitHub', ['github.com'])];
		piiState.records = [makeVaultRecord('p:gh', 'e:gh', 'github pw')];
		const { container } = render(AutofillPrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://github.com/login', [
					{ kind: 'password', selector: '#pw' }
				]),
				onClose: () => {}
			}
		});
		// Should render the picker (1 record), not a warning
		expect(container.querySelector('.warning')).toBeNull();
		expect(container.querySelectorAll('.record-row').length).toBe(1);
	});

	it('matches a parent domain — example.com matches mail.example.com', () => {
		piiState.entities = [makeEntity('e:ex', 'Example', ['example.com'])];
		piiState.records = [makeVaultRecord('p:ex', 'e:ex', 'main')];
		const { container } = render(AutofillPrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://mail.example.com/login', [
					{ kind: 'password', selector: '#pw' }
				]),
				onClose: () => {}
			}
		});
		expect(container.querySelector('.warning')).toBeNull();
		expect(container.querySelectorAll('.record-row').length).toBe(1);
	});

	it('does NOT match a partial-suffix string — corp.example.com does not match by typo "ample.com"', () => {
		// Guards against a bug where naive `.endsWith()` could match attacker-style "evilexample.com" against "example.com".
		piiState.entities = [makeEntity('e:ex', 'Example', ['ample.com'])];
		piiState.records = [makeVaultRecord('p:ex', 'e:ex', 'leak')];
		const { container } = render(AutofillPrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://example.com/login', [
					{ kind: 'password', selector: '#pw' }
				]),
				onClose: () => {}
			}
		});
		expect(container.querySelector('.warning')?.textContent).toContain('No entity');
	});
});

describe('AutofillPrompt — fill action', () => {
	it('calls autofill_pii_record with the selected record + password selector and shows success', async () => {
		piiState.entities = [makeEntity('e:gh', 'GitHub', ['github.com'])];
		piiState.records = [makeVaultRecord('p:gh', 'e:gh', 'main github pw')];
		const autofill = vi.fn(() => undefined);
		mockTauriCommand('autofill_pii_record', autofill);
		mockTauriCommand('list_pii_records', () => piiState.records);

		const { container } = render(AutofillPrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://github.com/login', [
					{ kind: 'password', selector: '#login-password' }
				]),
				onClose: () => {}
			}
		});

		const radio = container.querySelector('input[type="radio"]') as HTMLInputElement;
		await fireEvent.click(radio);
		await tick();

		const fillBtn = Array.from(container.querySelectorAll('button')).find((b) =>
			b.textContent?.includes('Fill')
		) as HTMLButtonElement;
		await fireEvent.click(fillBtn);
		await tick();
		await tick();

		expect(autofill).toHaveBeenCalledTimes(1);
		expect(autofill).toHaveBeenCalledWith({
			recordId: 'p:gh',
			selector: '#login-password'
		});
		expect(container.querySelector('.success')?.textContent).toContain('Injected');
	});

	it('shows the error and does not crash when autofill fails', async () => {
		piiState.entities = [makeEntity('e:gh', 'GitHub', ['github.com'])];
		piiState.records = [makeVaultRecord('p:gh', 'e:gh', 'main')];
		mockTauriCommand('autofill_pii_record', () => {
			throw new Error('webview not open');
		});

		const { container } = render(AutofillPrompt, {
			props: {
				open: true,
				extraction: makeExtraction('https://github.com/login', [
					{ kind: 'password', selector: '#pw' }
				]),
				onClose: () => {}
			}
		});

		const radio = container.querySelector('input[type="radio"]') as HTMLInputElement;
		await fireEvent.click(radio);
		await tick();
		const fillBtn = Array.from(container.querySelectorAll('button')).find((b) =>
			b.textContent?.includes('Fill')
		) as HTMLButtonElement;
		await fireEvent.click(fillBtn);
		await tick();
		await tick();

		expect(container.querySelector('.error')?.textContent).toContain('webview not open');
		expect(container.querySelector('.success')).toBeNull();
	});
});
