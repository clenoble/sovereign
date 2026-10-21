/** Rune-based reactive state for open document panels. */

import type { FullDocument, CommitSummary } from '$lib/api/commands';
import {
	getDocument,
	saveDocument as apiSave,
	closeDocument as apiClose,
	listCommits as apiListCommits,
	restoreCommit as apiRestoreCommit,
	SAVE_CONFLICT_PREFIX
} from '$lib/api/commands';

export interface OpenPanel {
	doc: FullDocument;
	dirty: boolean;
	position: { x: number; y: number };
	size: { width: number; height: number };
	zIndex: number;
	mode: 'edit' | 'preview' | 'history';
	commits: CommitSummary[];
	commitsLoaded: boolean;
	selectedCommit: number | null;
	skillsOverflowOpen: boolean;
	/** Why the last save was refused (shown in the panel), or null. */
	saveError: string | null;
	/** The document had changed elsewhere and this panel's text was saved over
	 *  it; the other version is in History (decided by Céline, 2026-10-12:
	 *  both versions kept, no question asked). Shown as information. */
	saveInfo: string | null;
	/** Saving over the other version failed too (it changed again): autosave
	 *  stops until the user keeps their text or loads the stored one. */
	conflict: boolean;
}

let nextZ = 100;
let openCount = 0;

/** Reactive array of open document panels. */
export const panels: OpenPanel[] = $state([]);

/** Open a document by ID. Prevents duplicates — brings existing to front. */
export async function openById(id: string) {
	const existing = panels.find((p) => p.doc.id === id);
	if (existing) {
		bringToFront(id);
		return;
	}
	try {
		const doc = await getDocument(id);
		const offset = openCount * 30;
		openCount++;
		panels.push({
			doc,
			dirty: false,
			position: { x: 120 + offset, y: 80 + offset },
			size: { width: 680, height: 520 },
			zIndex: nextZ++,
			mode: 'edit',
			commits: [],
			commitsLoaded: false,
			selectedCommit: null,
			skillsOverflowOpen: false,
			saveError: null,
			saveInfo: null,
			conflict: false
		});
	} catch (e) {
		console.error('Failed to open document:', e);
	}
}

/** Save the document to the backend (through the document service).
 *  `explicit`: the user asked for it (Ctrl+S, the Save button) — it always
 *  makes a version. A refused save keeps the text and says why in
 *  `saveError`. A conflict (the document changed elsewhere since it was read)
 *  keeps both versions without asking: this panel's text is saved on the
 *  current revision and the other version stays in History (`saveInfo` says
 *  so). Only if that save conflicts again does autosave stop until the user
 *  chooses (`keepMine` / `loadStored`). */
export async function save(id: string, explicit = false, retried = false) {
	const panel = panels.find((p) => p.doc.id === id);
	if (!panel || !panel.dirty || panel.conflict) return;
	const sent = { title: panel.doc.title, body: panel.doc.body };
	try {
		const stored = await apiSave(
			panel.doc.id,
			panel.doc.title,
			panel.doc.body,
			panel.doc.images,
			panel.doc.videos,
			panel.doc.revision,
			explicit
		);
		panel.doc.revision = stored.revision;
		panel.doc.modified_at = stored.modified_at;
		panel.saveError = null;
		// Nothing typed since the save was sent: show what is stored (personal
		// data replaced by tokens) so the next save does not send it again.
		if (panel.doc.title === sent.title && panel.doc.body === sent.body) {
			panel.doc.title = stored.title;
			panel.doc.body = stored.body;
			panel.dirty = false;
		}
	} catch (e) {
		const msg = String(e);
		if (msg.startsWith(SAVE_CONFLICT_PREFIX) && !retried) {
			try {
				const current = await getDocument(id);
				panel.doc.revision = current.revision;
			} catch (e2) {
				panel.saveError = String(e2);
				return;
			}
			panel.saveInfo =
				'This document had changed elsewhere. Your text was saved; the other version is in History.';
			await save(id, true, true);
		} else if (msg.startsWith(SAVE_CONFLICT_PREFIX)) {
			panel.conflict = true;
			panel.saveError = msg.slice(SAVE_CONFLICT_PREFIX.length);
		} else {
			panel.saveError = msg;
		}
	}
}

/** After a conflict: save this panel's text over the stored version (which
 *  stays in the document's History). */
export async function keepMine(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (!panel) return;
	try {
		const current = await getDocument(id);
		panel.doc.revision = current.revision;
		panel.conflict = false;
		panel.saveError = null;
		panel.dirty = true;
		await save(id, true);
	} catch (e) {
		panel.saveError = String(e);
	}
}

/** After a conflict: drop this panel's text and show the stored version. */
export async function loadStored(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (!panel) return;
	try {
		panel.doc = await getDocument(id);
		panel.dirty = false;
		panel.conflict = false;
		panel.saveError = null;
	} catch (e) {
		panel.saveError = String(e);
	}
}

/** Re-read an open document from the backend (after a skill wrote it). */
export async function refresh(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (!panel) return;
	try {
		panel.doc = await getDocument(id);
		panel.dirty = false;
		panel.conflict = false;
		panel.saveError = null;
	} catch (e) {
		console.error('Failed to reload document:', e);
	}
}

/** Hide the "saved over a version changed elsewhere" information. */
export function dismissSaveInfo(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (panel) panel.saveInfo = null;
}

/** Hide a save error that is not a conflict. */
export function dismissSaveError(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (panel && !panel.conflict) panel.saveError = null;
}

/** Close a panel. Saves first if dirty; a save that fails keeps the panel
 *  open (with its text and the reason) instead of closing over it. Then the
 *  editing session ends (its last state becomes a version). */
export async function close(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (!panel) return;
	if (panel.dirty) {
		await save(id);
		if (panel.dirty && (panel.saveError || panel.conflict)) return;
	}
	try {
		await apiClose(id);
	} catch (e) {
		console.error('close_document error:', e);
	}
	const idx = panels.findIndex((p) => p.doc.id === id);
	if (idx !== -1) panels.splice(idx, 1);
}

/** Update the body text. */
export function updateBody(id: string, body: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (panel) {
		panel.doc.body = body;
		panel.dirty = true;
	}
}

/** Update the title. */
export function updateTitle(id: string, title: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (panel) {
		panel.doc.title = title;
		panel.dirty = true;
	}
}

/** Bring a panel to the front. */
export function bringToFront(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (panel) {
		panel.zIndex = nextZ++;
	}
}

/** Update panel position (after drag). */
export function updatePosition(id: string, x: number, y: number) {
	const panel = panels.find((p) => p.doc.id === id);
	if (panel) {
		panel.position = { x, y };
	}
}

/** Switch panel mode (edit / preview / history). */
export function setMode(id: string, mode: 'edit' | 'preview' | 'history') {
	const panel = panels.find((p) => p.doc.id === id);
	if (panel) {
		panel.mode = mode;
	}
}

/** Lazy-load commits for history mode. */
export async function loadCommits(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (!panel || panel.commitsLoaded) return;
	try {
		const commits = await apiListCommits(id);
		panel.commits = commits;
		panel.commitsLoaded = true;
	} catch (e) {
		console.error('Failed to load commits:', e);
	}
}

/** Restore a document to a specific commit. */
export async function restoreVersion(id: string, commitId: string) {
	try {
		const doc = await apiRestoreCommit(id, commitId);
		const panel = panels.find((p) => p.doc.id === id);
		if (panel) {
			panel.doc = doc;
			panel.dirty = false;
			panel.mode = 'edit';
			panel.commitsLoaded = false;
		}
	} catch (e) {
		console.error('Failed to restore commit:', e);
	}
}

/** Ids print as `table:key` or `table:⟨key⟩` depending on the path. */
function sameVersion(a: string, b: string): boolean {
	const norm = (s: string) => s.replace(/[⟨⟩`]/g, '');
	return norm(a) === norm(b);
}

/** Open a document's History with one version selected — where "Use this
 *  version" puts it back (the ordinary version restore). Used by an alert's
 *  "Open the previous version". */
export async function openHistoryAt(id: string, commitId: string) {
	await openById(id);
	const panel = panels.find((p) => p.doc.id === id);
	if (!panel) return;
	bringToFront(id);
	panel.mode = 'history';
	panel.commitsLoaded = false;
	await loadCommits(id);
	const i = panel.commits.findIndex((c) => sameVersion(c.id, commitId));
	panel.selectedCommit = i >= 0 ? i : null;
}

/** Select a commit in history view. */
export function selectCommit(id: string, index: number | null) {
	const panel = panels.find((p) => p.doc.id === id);
	if (panel) {
		panel.selectedCommit = index;
	}
}

/** Toggle skills overflow dropdown. */
export function toggleSkillsOverflow(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (panel) {
		panel.skillsOverflowOpen = !panel.skillsOverflowOpen;
	}
}
