/** Guardian Access Recovery — pre-login wizard state (Surface 2). Svelte 5.
 *
 * Drives the access-recovery wizard over the real F1 commands. The user
 * forgot their password; their guardians release shares of the Recovery
 * Key; once `threshold` shares are in, the user sets a NEW passphrase and
 * the account is re-wrapped + unlocked. No old password, no data fragments.
 *
 * v0.1 is own-device: this device already holds the recovery card + bundle,
 * so `start` needs no card entry. State lives on disk backend-side, so the
 * wizard resumes across restarts (status() reads it on mount).
 */

import {
	accessRecoveryAvailable,
	startAccessRecovery,
	accessRecoveryPoll,
	accessRecoveryStatus,
	accessRecoveryFinalize,
	cancelAccessRecovery,
	type AccessRecoveryStatusDto
} from '$lib/api/commands';

// The multi-day wait is gated on guardian approval + the 72h window, not on
// our polling — a slow cadence is right.
const POLL_MS = 45_000;

/** Phases with nothing left to poll. */
const TERMINAL = new Set(['installed', 'failed']);

export const recovery = $state({
	visible: false,
	/** Whether this device can recover (holds a card + bundle). */
	available: false,
	status: null as AccessRecoveryStatusDto | null,
	starting: false,
	finalizing: false,
	/** A poll round is in flight (the UI says so, instead of looking stuck). */
	polling: false,
	/** Epoch ms of the next automatic round; null when nothing is scheduled.
	 *  The wizard counts down to this so the wait is legible. */
	nextPollAt: null as number | null,
	error: null as string | null
});

// Self-scheduling (setTimeout, not setInterval) so `nextPollAt` is always the
// truth: each round schedules the next one when it finishes.
let pollTimer: ReturnType<typeof setTimeout> | null = null;

/** Check whether the login screen should offer the recover option. */
export async function checkAvailable(): Promise<boolean> {
	try {
		recovery.available = await accessRecoveryAvailable();
	} catch {
		recovery.available = false;
	}
	return recovery.available;
}

export function openRecovery() {
	recovery.visible = true;
	recovery.error = null;
	// Resume: a recovery may already be in flight (state is on disk).
	resume();
}

export function closeRecovery() {
	recovery.visible = false;
	// The recovery keeps running backend-side; only the poll pauses.
	stopPolling();
}

/** Adopt a fresh DTO and stop the timer on a terminal phase. */
function apply(status: AccessRecoveryStatusDto | null) {
	if (!status) return;
	recovery.status = status;
	if (TERMINAL.has(status.phase)) stopPolling();
}

/** Cheap read (no network) — mount + resume. */
async function resume() {
	try {
		const status = await accessRecoveryStatus();
		if (status) {
			apply(status);
			if (!TERMINAL.has(status.phase)) startPolling();
		}
	} catch (e) {
		recovery.error = String(e);
	}
}

export async function start() {
	if (recovery.starting) return;
	recovery.starting = true;
	recovery.error = null;
	try {
		apply(await startAccessRecovery());
		startPolling();
	} catch (e) {
		recovery.error = String(e);
	}
	recovery.starting = false;
}

/** One network round, then schedule the next. Surfaces `polling` so the
 *  wizard can say "checking…" rather than sit silent for 45s. */
async function poll() {
	clearTimer();
	recovery.polling = true;
	recovery.nextPollAt = null;
	try {
		apply(await accessRecoveryPoll());
	} catch (e) {
		// Keep polling through transient errors; surface the text.
		recovery.error = String(e);
	}
	recovery.polling = false;
	scheduleNext();
}

/** Arm the next automatic round — unless the recovery is over. */
function scheduleNext() {
	clearTimer();
	const phase = recovery.status?.phase;
	if (phase && TERMINAL.has(phase)) {
		recovery.nextPollAt = null;
		return;
	}
	recovery.nextPollAt = Date.now() + POLL_MS;
	pollTimer = setTimeout(poll, POLL_MS);
}

/** Manual "Check now" — don't make the user wait out the timer after their
 *  guardians have just approved. Polls immediately and re-arms the cadence. */
export async function checkNow() {
	if (recovery.polling) return;
	await poll();
}

/** Reconstruct the Recovery Key from the shares and re-wrap the account
 *  under the user's NEW passphrase. On success the session is unlocked.
 *  Returns true when installed. */
export async function finalize(newPassphrase: string): Promise<boolean> {
	if (!newPassphrase || recovery.finalizing) return false;
	recovery.finalizing = true;
	recovery.error = null;
	try {
		const status = await accessRecoveryFinalize(newPassphrase);
		apply(status);
		recovery.finalizing = false;
		return status.phase === 'installed';
	} catch (e) {
		recovery.error = String(e);
		recovery.finalizing = false;
		return false;
	}
}

export async function cancel() {
	stopPolling();
	recovery.status = null;
	try {
		await cancelAccessRecovery();
	} catch {
		/* backend cleans up stale recoveries itself */
	}
}

/** Poll now, then keep the cadence going (each round arms the next). */
function startPolling() {
	poll();
}

function clearTimer() {
	if (pollTimer) {
		clearTimeout(pollTimer);
		pollTimer = null;
	}
}

function stopPolling() {
	clearTimer();
	recovery.nextPollAt = null;
}
