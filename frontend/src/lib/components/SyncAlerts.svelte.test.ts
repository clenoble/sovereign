import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import SyncAlerts from './SyncAlerts.svelte';
import { syncAlerts, reloadSyncAlerts, onPeerNeedsUpdate } from '$lib/stores/syncAlerts.svelte';

const alertDto = (id: string, prev: string | null) => ({
	doc_id: id,
	title: `Notes ${id}`,
	device: `peer-${id}`,
	device_name: 'Laptop',
	finding: 'injected instructions in the new content',
	previous_version: prev
});

beforeEach(() => {
	syncAlerts.alerts = [];
	syncAlerts.previousValues = [];
	syncAlerts.olderDevices = [];
	syncAlerts.open = false;
});

async function withAlerts(alerts: ReturnType<typeof alertDto>[], previous: unknown[] = []) {
	mockTauriCommand('list_sync_alerts', () => alerts);
	mockTauriCommand('list_previous_values', () => previous);
	await reloadSyncAlerts();
	await tick();
}

describe('SyncAlerts — the notice', () => {
	it('shows nothing when there is nothing to say', () => {
		const { container } = render(SyncAlerts);
		expect(container.querySelector('[role="alert"]')).toBeNull();
		expect(container.querySelector('[role="status"]')).toBeNull();
	});

	it('announces an alert as an alert, naming the device and the document', async () => {
		const { container } = render(SyncAlerts);
		await withAlerts([alertDto('a', 'commit:p')]);
		const n = container.querySelector('[role="alert"]');
		expect(n?.textContent).toContain('A change from Laptop to “Notes a” looks suspicious');
	});

	it('names a device on an older version in a status notice', async () => {
		const { container } = render(SyncAlerts, { props: { variant: 'mobile' } });
		onPeerNeedsUpdate('p1', 'Phone');
		await tick();
		const s = container.querySelector('[role="status"]');
		expect(s?.textContent).toContain('Phone runs an older version of Sovereign');
		expect(container.querySelector('.notices.mobile')).not.toBeNull();
	});
});

describe('SyncAlerts — the panel', () => {
	it('opens as a labelled dialog that takes focus, and closes on Escape', async () => {
		const { container, getByText } = render(SyncAlerts);
		await withAlerts([alertDto('a', 'commit:p')]);
		await fireEvent.click(getByText('See the alert'));
		await tick();
		const dlg = container.querySelector('[role="dialog"]') as HTMLElement;
		expect(dlg).not.toBeNull();
		expect(dlg.getAttribute('aria-labelledby')).toBe('sync-alerts-title');
		expect(document.activeElement).toBe(dlg);
		await fireEvent.keyDown(dlg, { key: 'Escape' });
		await tick();
		expect(container.querySelector('[role="dialog"]')).toBeNull();
	});

	it('offers "Open the previous version" only when there is one', async () => {
		const { getByText, queryAllByText } = render(SyncAlerts);
		await withAlerts([alertDto('a', 'commit:p'), alertDto('b', null)]);
		await fireEvent.click(getByText('See the alerts'));
		await tick();
		expect(queryAllByText('Open the document').length).toBe(2);
		expect(queryAllByText('Open the previous version').length).toBe(1);
	});

	it('dismiss removes the alert and changes nothing else', async () => {
		const dismissed = vi.fn();
		mockTauriCommand('dismiss_sync_alert', (args: { docId: string }) => dismissed(args.docId));
		const { getByText, getByLabelText, container } = render(SyncAlerts);
		await withAlerts([alertDto('a', 'commit:p'), alertDto('b', null)]);
		await fireEvent.click(getByText('See the alerts'));
		await tick();
		await fireEvent.click(getByLabelText('Dismiss the alert on Notes a'));
		await tick();
		expect(dismissed).toHaveBeenCalledWith('a');
		expect(syncAlerts.alerts.map((a) => a.docId)).toEqual(['b']);
		expect(container.querySelector('[role="dialog"]')).not.toBeNull();
	});

	it('lists previous values without a pending badge, restorable or removable', async () => {
		const restored = vi.fn();
		const forgotten = vi.fn();
		mockTauriCommand('restore_previous_value', (a: { id: string }) => restored(a.id));
		mockTauriCommand('forget_previous_value', (a: { id: string }) => forgotten(a.id));
		const { getByText, getByLabelText, container } = render(SyncAlerts);
		await withAlerts(
			[alertDto('a', null)],
			[
				{ id: 'r1', label: 'Thread: Work', device: 'p', device_name: 'Phone', at: '2026-10-15T10:00:00Z', can_restore: true },
				{ id: 'r2', label: 'A message', device: 'p', device_name: 'Phone', at: '2026-10-15T09:00:00Z', can_restore: false }
			]
		);
		await fireEvent.click(getByText('See the alert'));
		await tick();
		expect(getByText('Previous values')).not.toBeNull();
		expect(container.textContent?.toLowerCase()).not.toContain('pending');
		expect(container.textContent?.toLowerCase()).not.toContain('review');
		await fireEvent.click(getByLabelText('Restore the previous value of Thread: Work'));
		await fireEvent.click(getByLabelText('Remove A message from the list'));
		await tick();
		expect(restored).toHaveBeenCalledWith('r1');
		expect(forgotten).toHaveBeenCalledWith('r2');
		expect(syncAlerts.previousValues).toEqual([]);
	});
});
