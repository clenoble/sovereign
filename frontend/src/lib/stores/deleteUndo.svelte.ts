/** The one-click delete and its Undo notice on the phone (C-DOC adoption
 *  step 5, S5-8 and S5-11).
 *
 *  Spec ch. 03, *Soft delete and 30-day purge*, decided 2026-10-17 (Céline):
 *  "Deleting a document is one click; the notice says it is in the trash and
 *  can be restored for 30 days, and offers Undo." Plan: the notice reads
 *  "Moved to the trash — it can be restored for 30 days"; Undo restores it
 *  (`restore_from_trash`) while the notice is shown (**[choice]** 10 s).
 *  Times are passed in (milliseconds), so the window is testable.
 */

import { deleteDocument, restoreFromTrash } from '$lib/api/commands';

/** How long the notice and its Undo stay shown. **[choice]** */
export const UNDO_WINDOW_MS = 10_000;

export const NOTICE_TEXT = 'Moved to the trash — it can be restored for 30 days';

export interface UndoNotice {
	docId: string;
	text: string;
	shownAt: number;
}

export const undo = $state({
	notice: null as UndoNotice | null,
	/** The last refusal or failure, for the user. */
	error: null as string | null
});

function message(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

/** One click on Delete: moves `docId` to the trash and shows the notice. */
export async function deleteWithUndo(docId: string, now: number = Date.now()): Promise<void> {
	try {
		await deleteDocument(docId);
		undo.error = null;
		undo.notice = { docId, text: NOTICE_TEXT, shownAt: now };
	} catch (e) {
		undo.error = message(e);
	}
}

/** Whether the notice (and its Undo) is still shown at `now`. */
export function undoAvailable(now: number = Date.now()): boolean {
	return undo.notice !== null && now - undo.notice.shownAt < UNDO_WINDOW_MS;
}

/** Undo, clicked at `now`: restores the document while the notice is shown.
 *  `false` (nothing done, `undo.error` says why) once it is gone or when the
 *  document has left the trash. */
export async function undoDelete(now: number = Date.now()): Promise<boolean> {
	const notice = undo.notice;
	if (!notice || !undoAvailable(now)) {
		undo.notice = null;
		undo.error = 'Too late to undo here: the document is in Settings → Trash.';
		return false;
	}
	try {
		await restoreFromTrash(notice.docId, null);
		undo.notice = null;
		undo.error = null;
		return true;
	} catch (e) {
		undo.notice = null;
		undo.error = message(e);
		return false;
	}
}
