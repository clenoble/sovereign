/** Guardian Access Recovery — owner setup state (Surface 1). Svelte 5 store.
 *
 * Backs the Settings → Recovery panel: the 5-guardian roster and the
 * in-person enrollment flow, over the real F1 commands (list_guardians,
 * begin_guardian_enrollment). Guardians Shamir-split a Recovery Key that
 * wraps the account secrets — enrolling all 5 "arms" recovery.
 *
 * Poll-based: after arming an enrollment offer, the new guardian appears in
 * the roster on the next list_guardians() read (no event bridge in F1).
 */

import {
	listGuardians,
	beginGuardianEnrollment,
	type GuardianRosterDto,
	type BeginEnrollmentResult
} from '$lib/api/commands';

/** Recovery is usable only once all 5 guardians are enrolled. */
export const GUARDIAN_TOTAL = 5;
/** Any 3 of the 5 shares reconstruct the Recovery Key. */
export const GUARDIAN_THRESHOLD = 3;

export const recoverySetup = $state({
	roster: null as GuardianRosterDto | null,
	loading: false,
	loaded: false,
	error: null as string | null,
	/** The armed in-person enrollment offer, while the QR is showing. */
	offer: null as BeginEnrollmentResult | null,
	enrolling: false,
	offerError: null as string | null
});

export function enrolledCount(): number {
	return recoverySetup.roster?.enrolled_count ?? 0;
}

/** True once all 5 guardians are enrolled (recovery is usable). */
export function isArmed(): boolean {
	return recoverySetup.roster?.armed ?? false;
}

export async function refreshRoster() {
	recoverySetup.loading = !recoverySetup.loaded;
	recoverySetup.error = null;
	try {
		recoverySetup.roster = await listGuardians();
	} catch (e) {
		recoverySetup.error = String(e);
	}
	recoverySetup.loading = false;
	recoverySetup.loaded = true;
}

/** Arm an in-person enrollment offer for the next guardian. Returns true on
 *  success; the QR + spoken code then live in `recoverySetup.offer`. */
export async function beginEnrollment(): Promise<boolean> {
	if (recoverySetup.enrolling || isArmed()) return false;
	recoverySetup.enrolling = true;
	recoverySetup.offerError = null;
	recoverySetup.offer = null;
	try {
		recoverySetup.offer = await beginGuardianEnrollment();
		recoverySetup.enrolling = false;
		return true;
	} catch (e) {
		recoverySetup.offerError = String(e);
		recoverySetup.enrolling = false;
		return false;
	}
}

/** Dismiss the active enrollment offer (QR closed). */
export function clearOffer() {
	recoverySetup.offer = null;
	recoverySetup.offerError = null;
}
