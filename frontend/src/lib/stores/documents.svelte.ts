/** Rune-based reactive state for open document panels. */

import type { FullDocument, CommitSummary } from '$lib/api/commands';
import {
	getDocument,
	saveDocument as apiSave,
	closeDocument as apiClose,
	listCommits as apiListCommits,
	restoreCommit as apiRestoreCommit
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
			skillsOverflowOpen: false
		});
	} catch (e) {
		console.error('Failed to open document:', e);
	}
}

/** Save the document to the backend. */
export async function save(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (!panel || !panel.dirty) return;
	try {
		await apiSave(
			panel.doc.id,
			panel.doc.title,
			panel.doc.body,
			panel.doc.images,
			panel.doc.videos
		);
		panel.dirty = false;
	} catch (e) {
		console.error('Failed to save document:', e);
	}
}

/** Close a panel. Auto-saves if dirty, then flushes autocommit. */
export async function close(id: string) {
	const panel = panels.find((p) => p.doc.id === id);
	if (!panel) return;
	if (panel.dirty) {
		await save(id);
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
