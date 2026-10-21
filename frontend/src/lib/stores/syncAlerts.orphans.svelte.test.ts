/** S5-12 W5/W6 — the phone's store for documents whose thread was deleted
 *  (`doc/plans/C-DOC-adoption.md`, *S5-12*): loaded with the alerts, a notice
 *  that says how many wait, and nothing changed in the peer alerts. Written
 *  before the code by another author. */
import { beforeEach, describe, expect, it } from 'vitest';
import { mockTauriCommand } from '$lib/test/tauri';
import {
	orphanFromDto,
	orphanNotice,
	orphansNotice,
	reloadSyncAlerts,
	setOrphans,
	noticeText,
	syncAlerts
} from './syncAlerts.svelte';

const MARKER = 'zebra-orphan-text-12';

const orphanDto = (id: string, title: string, threadId: string, old: string | null, deviceName = 'Laptop') => ({
	doc_id: id,
	title,
	thread_id: threadId,
	deleted_thread_name: old,
	device: `peer-${id}`,
	device_name: deviceName
});

const alertDto = {
	doc_id: 'document:x',
	title: 'Notes x',
	device: 'peer-x',
	device_name: 'Laptop',
	finding: 'injected instructions in the new content',
	previous_version: null
};

beforeEach(() => {
	syncAlerts.alerts = [];
	syncAlerts.previousValues = [];
	syncAlerts.olderDevices = [];
	syncAlerts.orphans = [];
	syncAlerts.open = false;
});

describe('documents whose thread was deleted — the store', () => {
	it('reads a line from the host, naming the device and the old thread', () => {
		const o = orphanFromDto(orphanDto('document:a', 'Alpha', 'thread:gone', 'Old drafts'));
		expect(o).toEqual({ docId: 'document:a', title: 'Alpha', threadId: 'thread:gone', oldThreadName: 'Old drafts', device: 'Laptop' });
		expect(orphanFromDto(orphanDto('document:b', 'B', 'thread:gone', null, '')).device).toBe('peer-document:b');
		expect(orphanFromDto(orphanDto('document:b', 'B', 'thread:gone', null, '')).oldThreadName).toBe('');
	});

	it('the line says which document and which deleted thread, and no text', () => {
		const n = orphanNotice(orphanFromDto(orphanDto('document:a', 'Alpha', 'thread:gone', 'Old drafts')));
		expect(n).toContain('Alpha');
		expect(n).toContain('Old drafts');
		expect(n.toLowerCase()).toContain('deleted');
		expect(n).not.toContain(MARKER);
	});

	it('is loaded with the alerts, and leaves the alerts as they are', async () => {
		mockTauriCommand('list_sync_alerts', () => [alertDto]);
		mockTauriCommand('list_previous_values', () => []);
		mockTauriCommand('list_orphaned_documents', () => [
			orphanDto('document:a', 'Alpha', 'thread:gone', 'Old drafts'),
			orphanDto('document:b', 'Beta', 'thread:gone', 'Old drafts')
		]);
		await reloadSyncAlerts();
		expect(syncAlerts.orphans.map((o) => o.docId)).toEqual(['document:a', 'document:b']);
		expect(syncAlerts.alerts.map((a) => a.docId)).toEqual(['document:x']);
		expect(noticeText()).toBe('A change from Laptop to “Notes x” looks suspicious');
	});

	it('a host that cannot list them leaves the alerts loading as before', async () => {
		mockTauriCommand('list_sync_alerts', () => [alertDto]);
		mockTauriCommand('list_previous_values', () => []);
		// list_orphaned_documents not mocked: the call fails.
		await reloadSyncAlerts();
		expect(syncAlerts.alerts.length).toBe(1);
	});

	it('the notice says how many documents wait, and nothing when none do', () => {
		expect(orphansNotice()).toBeNull();
		setOrphans([orphanFromDto(orphanDto('document:a', 'Alpha', 'thread:gone', 'Old drafts'))]);
		const one = orphansNotice();
		expect(one).not.toBeNull();
		expect(one).toContain('Alpha');
		setOrphans([
			orphanFromDto(orphanDto('document:a', 'Alpha', 'thread:gone', 'Old drafts')),
			orphanFromDto(orphanDto('document:b', 'Beta', 'thread:gone', 'Old drafts')),
			orphanFromDto(orphanDto('document:c', 'Gamma', 'thread:other', 'Other'))
		]);
		expect(orphansNotice()).toContain('3');
		setOrphans([]);
		expect(orphansNotice()).toBeNull();
	});

	it('the peer-alert notice is not changed by documents waiting (W6)', () => {
		setOrphans([orphanFromDto(orphanDto('document:a', 'Alpha', 'thread:gone', 'Old drafts'))]);
		expect(noticeText()).toBeNull();
	});
});
