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
	historiesNotRead,
	listOrphanedDocuments,
	listPreviousValues,
	listSyncAlerts,
	listUnreadableDocuments,
	type OrphanDto,
	type PreviousValueDto,
	type SyncAlertDto,
	type UnreadableDocumentsDto
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
	/** Paired devices on a newer version: this device is the one to update
	 *  (C-WIRE 12 from the other side). */
	newerDevices: [] as OlderDevice[],
	/** Documents whose thread was deleted, waiting for a decision (S5-12). */
	orphans: [] as Orphan[],
	/** When the alerts, or the documents whose thread was deleted, could not
	 *  be read: the sentence said in their place, worded by the backend
	 *  (spec ch. 03, *What cannot be read is never shown as empty*); null when
	 *  they were read. The documents that could not be read carry theirs in
	 *  `unreadable.could_not_check`. */
	alertsCouldNotCheck: null as string | null,
	orphansCouldNotCheck: null as string | null,
	/** Item C5 (spec ch. 03, HR3): the backend's sentence for the documents
	 *  whose history this session could not read; null when none, or when the
	 *  count was refused (a locked session). */
	historiesNotRead: null as string | null,
	/** Documents the one-time conversion could not read (v0.0.12 item 7 V2b),
	 *  worded by the backend; empty: `''`, `''`, `[]`. */
	unreadable: { heading: '', meaning: '', lines: [] } as UnreadableDocumentsDto,
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

/** The older-version notice naming the device(s), or null. When a paired
 *  device runs a newer version, the notice says that this device is the one
 *  to update (C-WIRE 12 from the other side; wording [choice]). */
export function olderNotice(): string | null {
	const parts: string[] = [];
	const n = syncAlerts.newerDevices;
	if (n.length > 0)
		parts.push(
			`This device runs an older version of Sovereign than ${n.map((x) => x.name).join(', ')}. Update this device to sync documents with ${n.length === 1 ? 'it' : 'them'}; other data still syncs.`
		);
	const d = syncAlerts.olderDevices;
	if (d.length === 1)
		parts.push(`${d[0].name} runs an older version of Sovereign. Update it to sync documents with it; other data still syncs.`);
	if (d.length > 1)
		parts.push(
			`${d.map((x) => x.name).join(', ')} run an older version of Sovereign. Update them to sync documents; other data still syncs.`
		);
	return parts.length > 0 ? parts.join(' ') : null;
}

/** Nothing left to show in the Alerts dialog. */
function nothingToShow(): boolean {
	return (
		syncAlerts.alerts.length === 0 &&
		syncAlerts.previousValues.length === 0 &&
		syncAlerts.orphans.length === 0 &&
		syncAlerts.unreadable.lines.length === 0 &&
		couldNotCheck().length === 0
	);
}

/** The sentences of the lists that could not be read, in the dialog's order
 *  (spec ch. 03): a failure counts as something to show. */
export function couldNotCheck(): string[] {
	return [syncAlerts.alertsCouldNotCheck, syncAlerts.orphansCouldNotCheck, syncAlerts.unreadable.could_not_check].filter(
		(s): s is string => !!s
	);
}

export function setAlerts(list: SyncAlert[]) {
	syncAlerts.alerts = list;
	if (nothingToShow()) syncAlerts.open = false;
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

/** A paired device runs a newer version: this device is the one to update
 *  (the `this-device-needs-update` event). */
export function onThisDeviceNeedsUpdate(peerId: string, name: string) {
	roundSaidOlder.set(peerId, true);
	if (syncAlerts.newerDevices.some((d) => d.peerId === peerId)) return;
	syncAlerts.newerDevices = [...syncAlerts.newerDevices, { peerId, name: name || peerId }];
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
	if (said === false && syncAlerts.newerDevices.some((d) => d.peerId === peerId)) {
		syncAlerts.newerDevices = syncAlerts.newerDevices.filter((d) => d.peerId !== peerId);
	}
}

/** Re-reads the alerts, the previous values, the orphans and the documents
 *  that could not be read (after login, a completed sync, or a background
 *  model verdict). Best-effort: a locked workspace leaves the lists as they
 *  are, except the last, which it empties (V2b, B5). */
export async function reloadSyncAlerts() {
	try {
		const dto = await listSyncAlerts();
		syncAlerts.alertsCouldNotCheck = dto.could_not_check ?? null;
		syncAlerts.alerts = dto.alerts.map(alertFromDto);
	} catch {
		/* locked, or no document service: the list stays as it is, but a
		   sentence read under another session is not kept (spec ch. 03). */
		syncAlerts.alertsCouldNotCheck = null;
	}
	try {
		setPreviousValues((await listPreviousValues()).map(previousValueFromDto));
	} catch {
		/* locked */
	}
	await readOrphans();
	await readUnreadable();
	try {
		syncAlerts.historiesNotRead = (await historiesNotRead()).sentence ?? null;
	} catch {
		/* locked: a sentence read under another session is not kept */
		syncAlerts.historiesNotRead = null;
	}
	// Whether anything is left to show is decided once every list is read:
	// a list read later may hold something, or say it could not be read.
	closeIfNothingToShow();
}

function closeIfNothingToShow() {
	if (nothingToShow()) syncAlerts.open = false;
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
	if (nothingToShow()) syncAlerts.open = false;
}

/** Re-reads the documents whose thread was deleted. Best-effort: a locked
 *  workspace, or a host without the command, leaves the list as it is. */
export async function reloadOrphans() {
	await readOrphans();
	closeIfNothingToShow();
}

async function readOrphans() {
	try {
		const dto = await listOrphanedDocuments();
		syncAlerts.orphansCouldNotCheck = dto.could_not_check ?? null;
		syncAlerts.orphans = dto.orphans.map(orphanFromDto);
	} catch {
		/* locked, or a host that cannot list them: as for the alerts */
		syncAlerts.orphansCouldNotCheck = null;
	}
}

// ---- v0.0.12 item 7 V2b: documents the conversion could not read -------
// Kept as they were; Sovereign tries them again after its next update, so
// nothing here is acted on. Tests: `syncAlerts.unreadable.svelte.test.ts`,
// `SyncAlerts.unreadable.svelte.test.ts`.

/** "N document(s) could not be read" while any remain (B4), or null. */
export function unreadableNotice(): string | null {
	const n = syncAlerts.unreadable.lines.length;
	if (n === 0) return null;
	return n === 1 ? '1 document could not be read' : `${n} documents could not be read`;
}

export function setUnreadable(list: UnreadableDocumentsDto) {
	syncAlerts.unreadable = list;
	if (nothingToShow()) syncAlerts.open = false;
}

/** Re-reads the list (B5). Unlike the orphans, a refused or failed call
 *  empties it: a list read under another session never stays on screen
 *  (contract, "Readings settled" 1–2). */
export async function reloadUnreadable() {
	await readUnreadable();
	closeIfNothingToShow();
}

async function readUnreadable() {
	try {
		syncAlerts.unreadable = await listUnreadableDocuments();
	} catch {
		/* locked, or a host without the command */
		syncAlerts.unreadable = { heading: '', meaning: '', lines: [] };
	}
}

export function openAlerts() {
	syncAlerts.open = true;
}

export function closeAlerts() {
	syncAlerts.open = false;
}
