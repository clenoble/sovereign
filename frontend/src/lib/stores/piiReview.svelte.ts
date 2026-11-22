/** C-SWEEP on the phone (spec ch. 09, lot LS(a)): the review window grouped
 *  by type (SW5), the queue's signal and the notice at login (SW6).
 *
 *  - The window groups every record by type; each row says its state in
 *    words (`PII suspected - TBC`, `PII confirmed`, `PII dismissed`; Céline,
 *    2026-11-17). Checkboxes select one record or several; the gestures act on
 *    the selection, never on a whole type (Céline, 2026-11-17).
 *  - The status: `{n} personal data to review` while suspected values wait.
 *  - The notice at login, in the Fable maille's words (role `status` on the
 *    phone), shown only when no other notice is due: it yields to the setup
 *    notice, the migration notice and the sync-stopped notice.
 *
 *  Signatures by the test author of lot LS(a); the bodies are the
 *  implementer's. */

import {
	getPiiSignal,
	listPiiReviewGroups,
	reviewPiiRecords,
	takePiiLoginNotice,
	type ReviewGesture,
	type ReviewGroupDto
} from '$lib/api/commands';
import { setupNotice } from './setupNotice.svelte';
import { migrationNotice } from './migrationNotice.svelte';
import { syncStopped } from './syncStopped.svelte';

export const piiReview = $state({
	groups: [] as ReviewGroupDto[],
	/** The record ids selected by their checkboxes. */
	selected: [] as string[],
	loaded: false
});

export const piiSignal = $state({
	/** Suspected values waiting. */
	waiting: 0
});

export const piiLoginNotice = $state({
	title: null as string | null,
	body: null as string | null
});

/** Loads the review window (`pii_review_groups`). */
export async function loadReviewGroups(): Promise<void> {
	piiReview.groups = await listPiiReviewGroups();
	// A selection names only records still in the window.
	const shown = new Set(piiReview.groups.flatMap((g) => g.rows.map((r) => r.record_id)));
	piiReview.selected = piiReview.selected.filter((id) => shown.has(id));
	piiReview.loaded = true;
}

/** Selects or unselects one record. */
export function toggleSelected(recordId: string): void {
	if (piiReview.selected.includes(recordId)) {
		piiReview.selected = piiReview.selected.filter((id) => id !== recordId);
	} else {
		piiReview.selected = [...piiReview.selected, recordId];
	}
}

/** One gesture on the selection, in the order the rows were selected; then
 *  the selection is cleared and the window and the signal are read again.
 *  Nothing is sent when nothing is selected. */
async function onSelected(gesture: ReviewGesture): Promise<void> {
	const ids = [...piiReview.selected];
	if (ids.length === 0) return;
	await reviewPiiRecords(gesture, ids);
	piiReview.selected = [];
	await Promise.all([loadReviewGroups(), loadPiiSignal()]);
}

/** Confirms the records selected (`pii_review`, "confirm"), clears the
 *  selection and reloads the window and the signal. */
export async function confirmSelected(): Promise<void> {
	await onSelected('confirm');
}

/** Dismisses the records selected (`pii_review`, "dismiss"), as above. */
export async function dismissSelected(): Promise<void> {
	await onSelected('dismiss');
}

/** Takes back the dismissal of the records selected. */
export async function takeBackDismissalSelected(): Promise<void> {
	await onSelected('take_back_dismissal');
}

/** Takes back the confirmation of the records selected. */
export async function takeBackConfirmationSelected(): Promise<void> {
	await onSelected('take_back_confirmation');
}

/** Loads the queue's signal (`pii_signal`). */
export async function loadPiiSignal(): Promise<void> {
	const signal = await getPiiSignal();
	piiSignal.waiting = signal.waiting;
}

/** The status words for `piiSignal.waiting`: `{n} personal data to review`,
 *  or null when none waits. */
export function piiStatusWords(): string | null {
	return piiSignal.waiting > 0 ? `${piiSignal.waiting} personal data to review` : null;
}

/** Whether another notice is due at this login: the setup notice, the
 *  migration notice or the sync-stopped notice. */
function anotherNoticeIsDue(): boolean {
	return setupNotice.visible || migrationNotice.text !== null || syncStopped.noticeShown;
}

/** After a login: asks the backend once (`pii_login_notice`) and shows its
 *  notice only if no other notice is due. The backend renews the count seen
 *  at the last login either way. */
export async function offerPiiLoginNotice(): Promise<void> {
	const notice = await takePiiLoginNotice();
	if (!notice || anotherNoticeIsDue()) return;
	piiLoginNotice.title = notice.title;
	piiLoginNotice.body = notice.body;
}

/** The person closed the notice. */
export function dismissPiiLoginNotice(): void {
	piiLoginNotice.title = null;
	piiLoginNotice.body = null;
}
