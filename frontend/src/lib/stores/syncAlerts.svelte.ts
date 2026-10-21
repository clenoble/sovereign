/** Alerts on changes from paired devices, previous values of synced items,
 *  and devices on an older version — Svelte 5 rune store.
 *
 *  Spec ch. 05, C-WIRE 12–14 (decided by Céline, 2026-10-15): a change from
 *  one of your devices applies on its own; every received document change is
 *  analysed and only one that looks suspicious is listed here, until you
 *  dismiss it (dismissing changes nothing in the document). Going back is the
 *  ordinary "Use this version" in the document's History. A device on an
 *  older version exchanges no documents until it is updated; other data
 *  still syncs.
 */

import {
	listOrphanedDocuments,
	listPreviousValues,
	listSyncAlerts,
	type OrphanDto,
	type PreviousValueDto,
	type SyncAlertDto
} from '$lib/api/commands';

export interface SyncAlert {
	docId: string;
	title: string;
	/** The device's name, or its id when it is not a paired device. */
	device: string;
	finding: string;
	previousVersion: string | null;
}

export interface PreviousValue {
	id: string;
	label: string;
	device: string;
	at: string;
	canRestore: boolean;
}

export interface OlderDevice {
	peerId: string;
	name: string;
}

/** A document whose thread was deleted (S5-12): it waits for a decision. */
export interface Orphan {
	docId: string;
	title: string;
	/** The deleted thread it is still in. */
	threadId: string;
	/** The deleted thread's old name; empty when unknown. */
	oldThreadName: string;
	/** The device's name, or its id when it is not a paired device. */
	device: string;
}

export function alertFromDto(d: SyncAlertDto): SyncAlert {
	return {
		docId: d.doc_id,
		title: d.title,
		device: d.device_name || d.device,
		finding: d.finding,
		previousVersion: d.previous_version
	};
}

export function previousValueFromDto(d: PreviousValueDto): PreviousValue {
	return {
		id: d.id,
		label: d.label,
		device: d.device_name || d.device,
		at: d.at,
		canRestore: d.can_restore
	};
}

export const syncAlerts = $state({
	alerts: [] as SyncAlert[],
	previousValues: [] as PreviousValue[],
	olderDevices: [] as OlderDevice[],
	/** Documents whose thread was deleted, waiting for a decision (S5-12). */
	orphans: [] as Orphan[],
	/** The Alerts panel (desktop) or sheet (mobile) is open. */
	open: false
});

/** "A change from <device> to “<document>” looks suspicious". */
export function alertNotice(a: SyncAlert): string {
	const device = a.device || 'a paired device';
	const title = a.title || 'a document';
	return `A change from ${device} to “${title}” looks suspicious`;
}

/** The one-line notice for the banner, or null when there is nothing to say. */
export function noticeText(): string | null {
	const n = syncAlerts.alerts.length;
	if (n === 1) return alertNotice(syncAlerts.alerts[0]);
	if (n > 1) return `${n} changes from your devices look suspicious`;
	return null;
}

/** The older-version notice naming the device(s), or null. */
export function olderNotice(): string | null {
	const d = syncAlerts.olderDevices;
	if (d.length === 1)
		return `${d[0].name} runs an older version of Sovereign. Update it to sync documents with it; other data still syncs.`;
	if (d.length > 1)
		return `${d.map((x) => x.name).join(', ')} run an older version of Sovereign. Update them to sync documents; other data still syncs.`;
	return null;
}

export function setAlerts(list: SyncAlert[]) {
	syncAlerts.alerts = list;
	if (list.length === 0 && syncAlerts.previousValues.length === 0 && syncAlerts.orphans.length === 0)
		syncAlerts.open = false;
}

export function removeAlert(docId: string) {
	setAlerts(syncAlerts.alerts.filter((a) => a.docId !== docId));
}

export function setPreviousValues(list: PreviousValue[]) {
	syncAlerts.previousValues = list;
}

export function removePreviousValue(id: string) {
	syncAlerts.previousValues = syncAlerts.previousValues.filter((v) => v.id !== id);
}

/** Per peer, during a sync round: whether the round said "older version". */
const roundSaidOlder = new Map<string, boolean>();

/** A paired device runs an older version (the `peer-needs-update` event). */
export function onPeerNeedsUpdate(peerId: string, name: string) {
	roundSaidOlder.set(peerId, true);
	if (syncAlerts.olderDevices.some((d) => d.peerId === peerId)) return;
	syncAlerts.olderDevices = [...syncAlerts.olderDevices, { peerId, name: name || peerId }];
}

/** A sync round with `peerId` starts. */
export function onSyncRoundStarted(peerId: string) {
	roundSaidOlder.set(peerId, false);
}

/** A sync round with `peerId` completed: a device the round no longer called
 *  older has been updated, and its notice goes away (no flicker meanwhile). */
export function onSyncRoundCompleted(peerId: string) {
	const said = roundSaidOlder.get(peerId);
	roundSaidOlder.delete(peerId);
	if (said === false && syncAlerts.olderDevices.some((d) => d.peerId === peerId)) {
		syncAlerts.olderDevices = syncAlerts.olderDevices.filter((d) => d.peerId !== peerId);
	}
}

/** Re-reads the alerts and the previous values (after login, a completed
 *  sync, or a background model verdict). Best-effort: a locked workspace
 *  leaves the lists as they are. */
export async function reloadSyncAlerts() {
	try {
		setAlerts((await listSyncAlerts()).map(alertFromDto));
	} catch {
		/* locked, or no document service */
	}
	try {
		setPreviousValues((await listPreviousValues()).map(previousValueFromDto));
	} catch {
		/* locked */
	}
	await reloadOrphans();
}

// ---- S5-12: documents whose thread was deleted --------------------------
// A live document still in a thread deleted here (or whose thread's deletion
// arrived by sync) waits for a decision: move it, put it in a new thread, or
// put it in the trash. The document service computes the list; it is never a
// stored alert, and it changes nothing in the peer alerts above (W6). Tests:
// `syncAlerts.orphans.svelte.test.ts`, `SyncAlerts.orphans.svelte.test.ts`.

export function orphanFromDto(d: OrphanDto): Orphan {
	return {
		docId: d.doc_id,
		title: d.title,
		threadId: d.thread_id,
		oldThreadName: d.deleted_thread_name ?? '',
		device: d.device_name || d.device
	};
}

/** "<title> — its thread <old name> was deleted." (no document text). */
export function orphanNotice(o: Orphan): string {
	const title = o.title || 'Untitled';
	return o.oldThreadName
		? `“${title}” — its thread “${o.oldThreadName}” was deleted.`
		: `“${title}” — its thread was deleted.`;
}

/** The one-line notice saying how many documents wait, or null when none. */
export function orphansNotice(): string | null {
	const list = syncAlerts.orphans;
	if (list.length === 0) return null;
	if (list.length === 1) {
		const o = list[0];
		const title = o.title || 'Untitled';
		return o.oldThreadName
			? `“${title}” waits for a decision: its thread “${o.oldThreadName}” was deleted`
			: `“${title}” waits for a decision: its thread was deleted`;
	}
	return `${list.length} documents wait for a decision: their thread was deleted`;
}

export function setOrphans(list: Orphan[]) {
	syncAlerts.orphans = list;
	if (list.length === 0 && syncAlerts.alerts.length === 0 && syncAlerts.previousValues.length === 0)
		syncAlerts.open = false;
}

/** Re-reads the documents whose thread was deleted. Best-effort: a locked
 *  workspace, or a host without the command, leaves the list as it is. */
export async function reloadOrphans() {
	try {
		setOrphans((await listOrphanedDocuments()).map(orphanFromDto));
	} catch {
		/* locked, or a host that cannot list them */
	}
}

export function openAlerts() {
	syncAlerts.open = true;
}

export function closeAlerts() {
	syncAlerts.open = false;
}
