/** The trash, reached from Settings — Svelte 5 rune store (C-DOC adoption
 *  step 5, S5-11 and S5-5..S5-7 on the phone).
 *
 *  Spec ch. 03, *Soft delete and 30-day purge*, decided 2026-10-17 (Céline):
 *  the trash is reached from Settings; emptying it is the only permanent
 *  delete and asks first; a document comes back into its thread, or, if its
 *  thread was deleted, into a thread the user chooses. In the trash, a deleted
 *  thread's documents are grouped and can be restored all at once.
 *  Commands: `list_trash`, `restore_from_trash` ({ id, threadId }),
 *  `empty_trash`.
 */

import { emptyTrash, listTrash, restoreFromTrash, type TrashEntryDto } from '$lib/api/commands';

export interface TrashLine {
	id: string;
	title: string;
	threadId: string;
	/** `null` when the thread was deleted. */
	threadName: string | null;
	/** The deleted thread's old name, when the thread was deleted. */
	deletedThreadName: string | null;
	trashedAt: string;
	daysLeft: number;
}

export const trash = $state({
	lines: [] as TrashLine[],
	/** "Empty trash" was clicked and its confirmation is showing. */
	emptyRequested: false,
	/** The last refusal or failure, for the user. */
	error: null as string | null
});

function toLine(d: TrashEntryDto): TrashLine {
	return {
		id: d.id,
		title: d.title,
		threadId: d.thread_id,
		threadName: d.thread_name,
		deletedThreadName: d.deleted_thread_name ?? null,
		trashedAt: d.trashed_at,
		daysLeft: d.days_left
	};
}

function message(e: unknown): string {
	return e instanceof Error ? e.message : String(e);
}

/** Reads the trash (newest first). */
export async function loadTrash(): Promise<void> {
	try {
		trash.lines = (await listTrash()).map(toLine);
	} catch (e) {
		trash.error = message(e);
	}
}

/** What a deleted thread is called in the trash: its old name, then
 *  " - deleted" (Céline, 2026-10-17). */
function deletedLabel(oldName: string | null): string {
	return oldName ? `${oldName} - deleted` : 'Deleted thread';
}

/** What the line shows for its thread: its name, or that it was deleted. */
export function threadLabel(line: TrashLine): string {
	return line.threadName ?? deletedLabel(line.deletedThreadName);
}

/** Restores `id` — into `threadId` when given; a document whose thread was
 *  deleted needs one, and without it nothing is restored (`false`, and
 *  `trash.error` says why). */
export async function restore(id: string, threadId?: string): Promise<boolean> {
	const line = trash.lines.find((l) => l.id === id);
	if (line && line.threadName === null && !threadId) {
		trash.error = 'Its thread was deleted: choose a thread to restore it into.';
		return false;
	}
	try {
		await restoreFromTrash(id, threadId ?? null);
		trash.error = null;
		return true;
	} catch (e) {
		trash.error = message(e);
		return false;
	} finally {
		await loadTrash();
	}
}

/** The documents of one deleted thread (Céline, 2026-10-17: "a deleted
 *  thread's documents are grouped"). */
export interface TrashGroup {
	threadId: string;
	/** The group's heading: says the thread was deleted. */
	label: string;
	ids: string[];
}

/** One group per deleted thread, its documents in the order of the lines.
 *  Documents whose thread exists are in no group. */
export function deletedThreadGroups(): TrashGroup[] {
	const groups: TrashGroup[] = [];
	for (const l of trash.lines) {
		if (l.threadName !== null) continue;
		const g = groups.find((x) => x.threadId === l.threadId);
		if (g) g.ids.push(l.id);
		else groups.push({ threadId: l.threadId, label: deletedLabel(l.deletedThreadName), ids: [l.id] });
	}
	return groups;
}

/** "Restore all into a thread…": every document of the deleted thread
 *  `threadId` back into `intoThreadId` (`restore_from_trash` with that
 *  thread). `false`, and `trash.error` says why, when refused; nothing is
 *  restored then. */
export async function restoreGroup(threadId: string, intoThreadId: string): Promise<boolean> {
	const ids = deletedThreadGroups().find((g) => g.threadId === threadId)?.ids ?? [];
	let restored = 0;
	let failure: string | null = null;
	for (const id of ids) {
		try {
			await restoreFromTrash(id, intoThreadId);
			restored += 1;
		} catch (e) {
			failure ??= message(e);
			// The target is refused the same way for every document: stop at
			// the first refusal rather than repeat it.
			if (restored === 0) break;
		}
	}
	await loadTrash();
	if (failure !== null) {
		trash.error = restored === 0 ? failure : `${restored} of ${ids.length} documents were restored. ${failure}`;
		return false;
	}
	trash.error = null;
	return true;
}

/** "Empty trash" clicked: asks first; nothing is purged yet. */
export function requestEmpty(): void {
	trash.emptyRequested = true;
}

/** The confirmation dismissed. */
export function cancelEmpty(): void {
	trash.emptyRequested = false;
}

/** The confirmation accepted: empties the trash; how many were purged, or
 *  `null` when no confirmation was showing (nothing purged). */
export async function confirmEmpty(): Promise<number | null> {
	if (!trash.emptyRequested) return null;
	trash.emptyRequested = false;
	try {
		const n = await emptyTrash();
		trash.error = null;
		return n;
	} catch (e) {
		trash.error = message(e);
		return null;
	} finally {
		await loadTrash();
	}
}
