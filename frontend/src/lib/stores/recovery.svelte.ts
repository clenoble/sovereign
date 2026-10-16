/** Guardian Access Recovery — pre-login wizard state (Surface 2). Svelte 5.
 *
 * Drives the access-recovery wizard over the F1 commands, conforming to the
 * frozen recovery session API (SEAM-A, spec ch. 06 "Recovery flow"):
 *
 *   start(new_passphrase) → poll … poll → finalize()      (fresh recovery)
 *   resume(passphrase)    → poll … poll → finalize()      (after a restart)
 *
 * The user forgot their password. They choose the NEW password FIRST, before
 * any guardian is contacted: the backend seals the shares it collects under
 * it (RECOVERY-001) and holds it in memory for the rest of the app run. The
 * recovery window spans days and the app does not stay alive across it, so
 * on a later launch the in-progress state is on disk but the session is not:
 * the wizard asks for that same password again (`resume`), and the backend
 * answers the frozen sentinel `"wrong-passphrase"` when it does not match.
 * `finalize` sends no passphrase — the backend uses the one held since
 * start/resume.
 *
 * v0.1 is own-device: this device already holds the recovery card + bundle,
 * so `start` needs no card entry.
 */

import {
	accessRecoveryAvailable,
	startAccessRecovery,
	resumeAccessRecovery,
	accessRecoveryPoll,
	accessRecoveryStatus,
	accessRecoveryFinalize,
	cancelAccessRecovery,
	validatePasswordPolicy,
	WRONG_PASSPHRASE,
	type AccessRecoveryStatusDto
} from '$lib/api/commands';

// The multi-day wait is gated on guardian approval + the 72h window, not on
// our polling — a slow cadence is right.
const POLL_MS = 45_000;

/** Phases with nothing left to poll. */
const TERMINAL = new Set(['installed', 'failed']);

/** The backend's error when poll/finalize run without a start/resume in this
 *  app run (access_recovery.rs). Matched by substring: it is a message, not
 *  a frozen sentinel like `"wrong-passphrase"`. */
const SESSION_NOT_OPEN = 'recovery session not open';

export const WRONG_PASSPHRASE_MESSAGE =
	"That's not the password you started recovery with. Try again.";

/** What the wizard shows. Derived from the on-disk status plus whether this
 *  app run holds the recovery session — never stored separately, so the two
 *  cannot disagree. */
export type RecoveryStep =
	| 'choose-passphrase' // nothing in progress: pick the new password, then start
	| 'resume' // in progress on disk, session not open: re-enter that password
	| 'awaiting' // session open, waiting for guardian shares
	| 'ready' // session open, enough shares: finalize
	| 'installed'
	| 'failed';

export function stepOf(
	status: AccessRecoveryStatusDto | null,
	sessionOpen: boolean
): RecoveryStep {
	if (!status) return 'choose-passphrase';
	// Terminal phases need no passphrase: a failed recovery is discarded with
	// cancel, an installed one is done.
	if (status.phase === 'installed') return 'installed';
	if (status.phase === 'failed') return 'failed';
	if (!sessionOpen) return 'resume';
	return status.phase === 'ready' ? 'ready' : 'awaiting';
}

export const recovery = $state({
	visible: false,
	/** Whether this device can recover (holds a card + bundle). */
	available: false,
	status: null as AccessRecoveryStatusDto | null,
	/** This app run holds the recovery session (seal key + new passphrase,
	 *  backend-side), opened by a successful start or resume. Poll and
	 *  finalize need it. Survives closing the wizard; not an app restart. */
	sessionOpen: false,
	starting: false,
	resuming: false,
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

function errText(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

function isSessionNotOpen(e: unknown): boolean {
	return errText(e).includes(SESSION_NOT_OPEN);
}

/** Only an open session waiting for shares has anything to poll. */
function shouldPoll(): boolean {
	return recovery.sessionOpen && recovery.status?.phase === 'awaiting_shares';
}

/** Check whether the login screen should offer the recover option. */
export async function checkAvailable(): Promise<boolean> {
	try {
		recovery.available = await accessRecoveryAvailable();
	} catch {
		recovery.available = false;
	}
	return recovery.available;
}

export async function openRecovery() {
	recovery.visible = true;
	recovery.error = null;
	await refreshStatus();
}

export function closeRecovery() {
	recovery.visible = false;
	recovery.error = null;
	// The recovery stays on disk and the session stays open backend-side for
	// this app run; only the poll pauses.
	stopPolling();
}

/** Adopt a fresh DTO and stop the timer once there is nothing to wait for. */
function apply(status: AccessRecoveryStatusDto | null) {
	if (!status) return;
	recovery.status = status;
	if (TERMINAL.has(status.phase)) {
		stopPolling();
		// The backend drops the session once installed (and the state is gone).
		if (status.phase === 'installed') recovery.sessionOpen = false;
	}
}

/** Cheap read (no network, no passphrase): what is on disk. Polling resumes
 *  only if this app run already holds the session; otherwise the wizard
 *  shows the resume step and waits for the passphrase. */
async function refreshStatus() {
	try {
		const status = await accessRecoveryStatus();
		recovery.status = status;
		// Nothing on disk: any session this run held belonged to a recovery
		// that is gone (installed or cancelled).
		if (!status) recovery.sessionOpen = false;
		else apply(status);
		if (shouldPoll()) startPolling();
	} catch (e) {
		recovery.error = errText(e);
	}
}

/** Fresh recovery. The new password is chosen first and checked here — never
 *  calls the backend without one (SEAM-A: `start(new_passphrase)`). It
 *  becomes the account password at finalize, so it meets the same policy as
 *  onboarding. Returns true when the recovery started. */
export async function start(newPassphrase: string, confirmation: string): Promise<boolean> {
	if (recovery.starting) return false;
	recovery.error = null;
	if (!newPassphrase) {
		recovery.error = 'Choose a new password first.';
		return false;
	}
	if (newPassphrase !== confirmation) {
		recovery.error = "The two passwords don't match.";
		return false;
	}
	recovery.starting = true;
	try {
		const policy = await validatePasswordPolicy(newPassphrase);
		if (!policy.valid) {
			recovery.error =
				policy.errors.length > 0
					? `Choose a stronger password: ${policy.errors.join(' ')}`
					: 'Choose a stronger password.';
			return false;
		}
		const status = await startAccessRecovery(newPassphrase);
		recovery.sessionOpen = true;
		apply(status);
		if (shouldPoll()) startPolling();
		return true;
	} catch (e) {
		recovery.error = `Couldn't start recovery: ${errText(e)}`;
		return false;
	} finally {
		recovery.starting = false;
	}
}

/** Re-open the in-progress recovery with the new password chosen at start
 *  (SEAM-A: `resume`). A wrong password is the frozen `"wrong-passphrase"`
 *  sentinel: re-prompt, don't alarm. Returns true when the session is open. */
export async function resume(passphrase: string): Promise<boolean> {
	if (recovery.resuming) return false;
	recovery.error = null;
	if (!passphrase) {
		recovery.error = 'Enter the new password you chose when you started.';
		return false;
	}
	recovery.resuming = true;
	try {
		const status = await resumeAccessRecovery(passphrase);
		recovery.sessionOpen = true;
		apply(status);
		if (shouldPoll()) startPolling();
		return true;
	} catch (e) {
		recovery.error =
			errText(e) === WRONG_PASSPHRASE
				? WRONG_PASSPHRASE_MESSAGE
				: `Couldn't resume recovery: ${errText(e)}`;
		return false;
	} finally {
		recovery.resuming = false;
	}
}

/** One network round, then schedule the next. Surfaces `polling` so the
 *  wizard can say "checking…" rather than sit silent for 45s. */
async function poll() {
	clearTimer();
	if (!recovery.sessionOpen) {
		recovery.nextPollAt = null;
		return;
	}
	recovery.polling = true;
	recovery.nextPollAt = null;
	try {
		apply(await accessRecoveryPoll());
	} catch (e) {
		if (isSessionNotOpen(e)) {
			// The backend lost the session: back to the resume step.
			recovery.sessionOpen = false;
			recovery.polling = false;
			stopPolling();
			return;
		}
		// Keep polling through transient errors; surface the text.
		recovery.error = errText(e);
	}
	recovery.polling = false;
	scheduleNext();
}

/** Arm the next automatic round — unless there is nothing left to wait for. */
function scheduleNext() {
	clearTimer();
	if (!shouldPoll()) {
		recovery.nextPollAt = null;
		return;
	}
	recovery.nextPollAt = Date.now() + POLL_MS;
	pollTimer = setTimeout(poll, POLL_MS);
}

/** Manual "Check now" — don't make the user wait out the timer after their
 *  guardians have just approved. Polls immediately and re-arms the cadence. */
export async function checkNow() {
	if (recovery.polling || !recovery.sessionOpen) return;
	await poll();
}

/** Reconstruct the Recovery Key from the shares and re-wrap the account
 *  under the new password held by the session since start/resume — no
 *  password is sent here. On success the session is unlocked. Returns true
 *  when installed. */
export async function finalize(): Promise<boolean> {
	if (recovery.finalizing || !recovery.sessionOpen) return false;
	recovery.finalizing = true;
	recovery.error = null;
	try {
		const status = await accessRecoveryFinalize();
		apply(status);
		return status.phase === 'installed';
	} catch (e) {
		if (isSessionNotOpen(e)) recovery.sessionOpen = false;
		recovery.error = errText(e);
		// A failed reconstruction or install is recorded on disk as phase
		// `failed`; re-read so the wizard shows it.
		try {
			apply(await accessRecoveryStatus());
		} catch {
			/* keep the error already shown */
		}
		return false;
	} finally {
		recovery.finalizing = false;
	}
}

export async function cancel() {
	stopPolling();
	recovery.status = null;
	recovery.sessionOpen = false;
	recovery.error = null;
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
