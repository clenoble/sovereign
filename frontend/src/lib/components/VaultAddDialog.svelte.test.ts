import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import VaultAddDialog from './VaultAddDialog.svelte';
import { piiState } from '$lib/stores/pii.svelte';
import type { PiiEntity, PiiRecord } from '$lib/api/commands';

function makeEntity(id: string, name: string): PiiEntity {
	return {
		id,
		name,
		kind: 'org',
		domains: [],
		contact_ids: [],
		notes: '',
		is_owned: false,
		created_at: '2026-04-01T00:00:00Z',
		modified_at: '2026-04-01T00:00:00Z'
	};
}

function makeRecord(id: string): PiiRecord {
	return {
		id,
		kind: 'password',
		label: null,
		entity_id: null,
		stored_secret: true,
		confidence: 1.0,
		review_state: 'confirmed',
		discovered_at: '2026-04-15T00:00:00Z',
		last_revealed_at: null,
		use_count: 0,
		sources: []
	};
}

beforeEach(() => {
	piiState.entities = [];
	piiState.records = [];
	piiState.selectedEntityId = null;
});

describe('VaultAddDialog — render gating', () => {
	it('renders nothing when open=false', () => {
		const { container } = render(VaultAddDialog, {
			props: { open: false, onClose: () => {} }
		});
		expect(container.querySelector('.vault-dialog')).toBeNull();
	});

	it('renders the dialog when open=true', () => {
		const { container } = render(VaultAddDialog, {
			props: { open: true, onClose: () => {} }
		});
		expect(container.querySelector('.vault-dialog')).not.toBeNull();
	});

	it('lists every PII entity in the entity dropdown', () => {
		piiState.entities = [makeEntity('e:1', 'Acme Bank'), makeEntity('e:2', 'GitHub')];
		const { container } = render(VaultAddDialog, {
			props: { open: true, onClose: () => {} }
		});
		// Two <select>s: [0] = kind, [1] = entity. Scope to the entity select.
		const entitySelect = container.querySelectorAll('select')[1] as HTMLSelectElement;
		const options = entitySelect.querySelectorAll('option');
		// "Unattributed" + 2 entities
		expect(options.length).toBe(3);
		expect(options[0].textContent).toBe('Unattributed');
		expect(options[1].textContent).toContain('Acme Bank');
		expect(options[2].textContent).toContain('GitHub');
	});
});

describe('VaultAddDialog — submit validation', () => {
	it('shows "Value is required" and skips the IPC call when value is empty', async () => {
		const onClose = vi.fn();
		// Register a handler so we can assert it WASN'T called
		const create = vi.fn(() => makeRecord('p:new'));
		mockTauriCommand('create_vault_entry', create);

		const { container } = render(VaultAddDialog, { props: { open: true, onClose } });
		const form = container.querySelector('form') as HTMLFormElement;
		await fireEvent.submit(form);
		await tick();

		expect(container.querySelector('.error')?.textContent).toContain('Value is required');
		expect(create).not.toHaveBeenCalled();
		expect(onClose).not.toHaveBeenCalled();
	});
});

describe('VaultAddDialog — submit happy path', () => {
	it('calls create_vault_entry with the form values, refreshes records, and closes', async () => {
		const onClose = vi.fn();
		const create = vi.fn((args: { input: { kind: string; label: string | null; entity_id: string | null; value: string } }) => {
			expect(args.input.kind).toBe('password');
			expect(args.input.value).toBe('Tr0ub4dor');
			return makeRecord('p:new');
		});
		mockTauriCommand('create_vault_entry', create);
		const refresh = vi.fn(() => [makeRecord('p:1'), makeRecord('p:new')]);
		mockTauriCommand('list_pii_records', refresh);

		const { container } = render(VaultAddDialog, { props: { open: true, onClose } });
		const valueInput = container.querySelector('input[type="password"]') as HTMLInputElement;
		await fireEvent.input(valueInput, { target: { value: 'Tr0ub4dor' } });

		const form = container.querySelector('form') as HTMLFormElement;
		await fireEvent.submit(form);
		await tick();
		await tick(); // second tick to drain the post-await refresh

		expect(create).toHaveBeenCalledTimes(1);
		expect(refresh).toHaveBeenCalledTimes(1);
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it('strips whitespace-only label to null before sending', async () => {
		mockTauriCommand('list_pii_records', () => []);
		const create = vi.fn((args: { input: { label: string | null } }) => {
			expect(args.input.label).toBeNull();
			return makeRecord('p:new');
		});
		mockTauriCommand('create_vault_entry', create);

		const { container } = render(VaultAddDialog, {
			props: { open: true, onClose: () => {} }
		});
		const inputs = container.querySelectorAll('input');
		const labelInput = inputs[0] as HTMLInputElement; // first text input is the label
		const valueInput = container.querySelector('input[type="password"]') as HTMLInputElement;
		await fireEvent.input(labelInput, { target: { value: '   ' } });
		await fireEvent.input(valueInput, { target: { value: 'secret' } });

		const form = container.querySelector('form') as HTMLFormElement;
		await fireEvent.submit(form);
		await tick();
		await tick();

		expect(create).toHaveBeenCalledTimes(1);
	});
});

describe('VaultAddDialog — submit error', () => {
	it('shows the error and does NOT close when create_vault_entry rejects', async () => {
		const onClose = vi.fn();
		mockTauriCommand('create_vault_entry', () => {
			throw new Error('encrypt failed');
		});

		const { container } = render(VaultAddDialog, { props: { open: true, onClose } });
		const valueInput = container.querySelector('input[type="password"]') as HTMLInputElement;
		await fireEvent.input(valueInput, { target: { value: 'secret' } });
		const form = container.querySelector('form') as HTMLFormElement;
		await fireEvent.submit(form);
		await tick();
		await tick();

		expect(container.querySelector('.error')?.textContent).toContain('encrypt failed');
		expect(onClose).not.toHaveBeenCalled();
	});
});

describe('VaultAddDialog — entity defaulting', () => {
	it('defaults the entity dropdown to defaultEntityId when provided', async () => {
		piiState.entities = [makeEntity('e:1', 'A'), makeEntity('e:2', 'B')];
		const { container } = render(VaultAddDialog, {
			props: { open: true, onClose: () => {}, defaultEntityId: 'e:2' }
		});
		await tick();
		const select = container.querySelectorAll('select')[1] as HTMLSelectElement;
		expect(select.value).toBe('e:2');
	});

	it('falls back to piiState.selectedEntityId when defaultEntityId is null', async () => {
		piiState.entities = [makeEntity('e:1', 'A')];
		piiState.selectedEntityId = 'e:1';
		const { container } = render(VaultAddDialog, {
			props: { open: true, onClose: () => {} }
		});
		await tick();
		const select = container.querySelectorAll('select')[1] as HTMLSelectElement;
		expect(select.value).toBe('e:1');
	});
});
