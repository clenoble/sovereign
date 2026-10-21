import { beforeEach, describe, expect, it } from 'vitest';
import {
	alertFromDto,
	alertNotice,
	closeAlerts,
	noticeText,
	olderNotice,
	onPeerNeedsUpdate,
	onSyncRoundCompleted,
	onSyncRoundStarted,
	openAlerts,
	previousValueFromDto,
	removeAlert,
	removePreviousValue,
	setAlerts,
	setPreviousValues,
	syncAlerts
} from './syncAlerts.svelte';

const dto = (id: string, name = 'Laptop', prev: string | null = 'commit:p') => ({
	doc_id: id,
	title: `Doc ${id}`,
	device: `peer-${id}`,
	device_name: name,
	finding: 'injected instructions in the new content',
	previous_version: prev
});

beforeEach(() => {
	syncAlerts.alerts = [];
	syncAlerts.previousValues = [];
	syncAlerts.olderDevices = [];
	syncAlerts.open = false;
});

describe('sync alerts', () => {
	it('names the device, or its id when it is not a paired device', () => {
		expect(alertFromDto(dto('a')).device).toBe('Laptop');
		expect(alertFromDto(dto('b', '')).device).toBe('peer-b');
		expect(alertFromDto(dto('c', 'Phone', null)).previousVersion).toBeNull();
	});

	it('says the notice in plain words', () => {
		expect(alertNotice(alertFromDto(dto('a')))).toBe('A change from Laptop to “Doc a” looks suspicious');
		expect(noticeText()).toBeNull();
		setAlerts([alertFromDto(dto('a'))]);
		expect(noticeText()).toBe('A change from Laptop to “Doc a” looks suspicious');
		setAlerts([alertFromDto(dto('a')), alertFromDto(dto('b'))]);
		expect(noticeText()).toBe('2 changes from your devices look suspicious');
	});

	it('removes a dismissed alert and closes the panel when nothing is left', () => {
		setAlerts([alertFromDto(dto('a')), alertFromDto(dto('b'))]);
		openAlerts();
		removeAlert('a');
		expect(syncAlerts.alerts.map((a) => a.docId)).toEqual(['b']);
		expect(syncAlerts.open).toBe(true);
		removeAlert('b');
		expect(syncAlerts.open).toBe(false);
		openAlerts();
		closeAlerts();
		expect(syncAlerts.open).toBe(false);
	});

	it('keeps previous values without a pending flag and removes one', () => {
		setPreviousValues([
			previousValueFromDto({ id: 'r1', label: 'Thread: Work', device: 'p', device_name: 'Phone', at: '2026-10-15T10:00:00Z', can_restore: true })
		]);
		expect(syncAlerts.previousValues[0]).toEqual({ id: 'r1', label: 'Thread: Work', device: 'Phone', at: '2026-10-15T10:00:00Z', canRestore: true });
		removePreviousValue('r1');
		expect(syncAlerts.previousValues).toEqual([]);
	});
});

describe('devices on an older version', () => {
	it('names the device and keeps one notice per device', () => {
		onPeerNeedsUpdate('p1', 'Laptop');
		onPeerNeedsUpdate('p1', 'Laptop');
		expect(syncAlerts.olderDevices).toEqual([{ peerId: 'p1', name: 'Laptop' }]);
		expect(olderNotice()).toBe(
			'Laptop runs an older version of Sovereign. Update it to sync documents with it; other data still syncs.'
		);
		onPeerNeedsUpdate('p2', '');
		expect(olderNotice()).toContain('Laptop, p2 run an older version');
	});

	it('stays through a round that still says so and goes after a round that does not', () => {
		onPeerNeedsUpdate('p1', 'Laptop');
		onSyncRoundStarted('p1');
		onPeerNeedsUpdate('p1', 'Laptop');
		onSyncRoundCompleted('p1');
		expect(syncAlerts.olderDevices.length).toBe(1);
		onSyncRoundStarted('p1');
		onSyncRoundCompleted('p1');
		expect(syncAlerts.olderDevices).toEqual([]);
		expect(olderNotice()).toBeNull();
	});
});
