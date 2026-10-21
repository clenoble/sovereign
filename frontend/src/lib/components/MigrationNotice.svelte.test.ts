/** C-DOC adoption step 6, M2 — the one-time migration's notice on the
 *  Tauri frontend: shown from the backend's text, until dismissed. */
import { beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { migrationNotice, showMigrationNotice } from '$lib/stores/migrationNotice.svelte';
import MigrationNotice from './MigrationNotice.svelte';

const TEXT = "3 documents converted to the new format — each one's earlier state is in its History.";

beforeEach(() => {
	migrationNotice.text = null;
});

describe('MigrationNotice', () => {
	it('shows nothing before the event', () => {
		const { container } = render(MigrationNotice);
		expect(container.querySelector('[role="status"]')).toBeNull();
	});

	it('shows the backend text when the event arrives', async () => {
		const { container } = render(MigrationNotice);
		showMigrationNotice(TEXT);
		await tick();
		expect(container.querySelector('[role="status"]')?.textContent).toContain(TEXT);
	});

	it('stays until dismissed, then is gone', async () => {
		const { container, getByRole } = render(MigrationNotice);
		showMigrationNotice(TEXT);
		await tick();
		await fireEvent.click(getByRole('button', { name: 'OK' }));
		await tick();
		expect(container.querySelector('[role="status"]')).toBeNull();
		expect(migrationNotice.text).toBeNull();
	});

	it('ignores an empty text', () => {
		showMigrationNotice('  ');
		expect(migrationNotice.text).toBeNull();
	});
});
