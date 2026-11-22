/** The node's supervision on the app built with P2P (contract C-SUP, spec
 *  ch. 05, *The node's supervision*): when the node ends without the session
 *  asking for it, the status line says « Sync stopped », and once per stop a
 *  notice says « Sync stopped unexpectedly. Nothing syncs until you restart
 *  it. », with the gesture « Restart sync » (SU2). A restart that fails says
 *  « Sync could not be restarted: {reason} ». An end the session asked for (a
 *  logout, a switch, a lock, the exit) says nothing (SU5).
 *
 *  Signatures by the C-SUP test author (lot LQ). The tests:
 *  `syncStopped.svelte.test.ts`. The words are the Fable maille's, by
 *  delegation of Céline (2026-11-15), the same as the shell's.
 */

import { restartSyncCmd } from '$lib/api/commands';

/** What the notice shows: its title, its text, and the gesture beside it. */
export interface SyncStoppedNotice {
	title: string;
	text: string;
	/** The gesture's label, present wherever the notice is shown. */
	gesture: string;
}

const STATUS = 'Sync stopped';
const TITLE = 'Sync stopped';
const TEXT = 'Sync stopped unexpectedly. Nothing syncs until you restart it.';
const GESTURE = 'Restart sync';
const FAILED = 'Sync could not be restarted: ';

/** The store's state, exported so that the AR4 reset at a switch (C-AREA,
 *  `$lib/sessionStores.svelte.ts`) brings it back to its initial value:
 *  a stop ends with its session (spec ch. 05, C-SUP SU4). Its initial value is
 *  « sync runs ». Every variable of the store lives here. */
export const syncStopped = $state({
	/** The node ended without the session asking for it, or did not start. */
	stopped: false,
	/** The stop's notice is shown. */
	noticeShown: false,
	/** The reason of a restart that failed, or null. */
	restartError: null as string | null
});

/** The stop's notice, with its gesture: shown by the stop itself, and again
 *  in answer to a gesture that needs the node while sync is stopped (the
 *  Fable maille, by delegation of Céline, 2026-11-15). */
export function showSyncStoppedNotice(): void {
	syncStopped.stopped = true;
	syncStopped.restartError = null;
	syncStopped.noticeShown = true;
}

/** Whether a command answered with the stop's text (a command that needs the
 *  node answers with it while sync is stopped). If so, the stop's notice is
 *  shown again, with its gesture. */
export function answeredSyncStopped(error: unknown): boolean {
	if (String(error).trim() !== TEXT) return false;
	showSyncStoppedNotice();
	return true;
}

/** The node ended without the session asking for it (the `sync-stopped` event). */
export function onSyncStopped(): void {
	showSyncStoppedNotice();
}

/** A restart the user asked for failed, with the reason the backend gave.
 *  Sync stays stopped, and the gesture stays beside the notice. */
export function onRestartFailed(reason: string): void {
	syncStopped.stopped = true;
	syncStopped.restartError = reason;
	syncStopped.noticeShown = true;
}

/** A restart the user asked for succeeded: a fresh node runs. */
export function onSyncRestarted(): void {
	syncStopped.stopped = false;
	syncStopped.noticeShown = false;
	syncStopped.restartError = null;
}

/** An end the session asked for (logout, switch, lock, exit): says nothing. */
export function onSessionEnded(): void {
	syncStopped.stopped = false;
	syncStopped.noticeShown = false;
	syncStopped.restartError = null;
}

/** The login's answer about the session it opens: after its reset
 *  (`resetSessionState()`), the login asks the backend whether this session's
 *  sync is stopped (the command `sync_is_stopped`, which reads the flag
 *  `node_refusal` reads) and hands the answer here. If it is stopped, the
 *  store takes the stop: the status line, and the notice once (spec ch. 05,
 *  C-SUP SU1: « The stop reaches the screen after the login has answered »;
 *  Céline, 2026-11-16). The `sync-stopped` event stays for a stop during the
 *  session. */
export function onSyncStateAtLogin(stopped: boolean): void {
	// A stop the event already brought after the reset is not shown twice.
	if (stopped && !syncStopped.stopped) showSyncStoppedNotice();
}

/** The user closed the notice. */
export function dismissSyncStoppedNotice(): void {
	syncStopped.noticeShown = false;
}

/** The status line's words, or null when sync has not stopped. */
export function syncStoppedStatus(): string | null {
	return syncStopped.stopped ? STATUS : null;
}

/** The notice to show now, or null. */
export function syncStoppedNotice(): SyncStoppedNotice | null {
	if (!syncStopped.noticeShown) return null;
	const text = syncStopped.restartError === null ? TEXT : `${FAILED}${syncStopped.restartError}`;
	return { title: TITLE, text, gesture: GESTURE };
}

/** DEL-0019 (the Fable maille, by delegation of Céline): the user made a
 *  gesture that needs the node. While sync is stopped, the stop's notice shows
 *  again, with its gesture, and this returns true (the screen asks nothing of
 *  the backend); otherwise it returns false and changes nothing. An answer to
 *  the user's own gesture is not the notice coming back by itself. */
export function onGestureNeedingTheNode(): boolean {
	if (!syncStopped.stopped) return false;
	showSyncStoppedNotice();
	return true;
}

/** The gesture « Restart sync »: asks the backend for a fresh node. */
export async function restartSync(): Promise<void> {
	try {
		await restartSyncCmd();
		onSyncRestarted();
	} catch (e) {
		onRestartFailed(String(e));
	}
}
