/** Store for open document panels. */

import { writable, get } from 'svelte/store';
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

function createDocumentsStore() {
	const { subscribe, update, set } = writable<OpenPanel[]>([]);

	return {
		subscribe,

		/** Open a document by ID. Prevents duplicates — brings existing to front. */
		async openById(id: string) {
			const current = get({ subscribe });
			const existing = current.find((p) => p.doc.id === id);
			if (existing) {
				this.bringToFront(id);
				return;
			}
			try {
				const doc = await getDocument(id);
				const offset = openCount * 30;
				openCount++;
				const panel: OpenPanel = {
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
				};
				update((panels) => [...panels, panel]);
			} catch (e) {
				console.error('Failed to open document:', e);
			}
		},

		/** Save the document to the backend. */
		async save(id: string) {
			const current = get({ subscribe });
			const panel = current.find((p) => p.doc.id === id);
			if (!panel || !panel.dirty) return;
			try {
				await apiSave(
					panel.doc.id,
					panel.doc.title,
					panel.doc.body,
					panel.doc.images,
					panel.doc.videos
				);
				update((panels) =>
					panels.map((p) => (p.doc.id === id ? { ...p, dirty: false } : p))
				);
			} catch (e) {
				console.error('Failed to save document:', e);
			}
		},

		/** Close a panel. Auto-saves if dirty, then flushes autocommit. */
		async close(id: string) {
			const current = get({ subscribe });
			const panel = current.find((p) => p.doc.id === id);
			if (!panel) return;
			if (panel.dirty) {
				await this.save(id);
			}
			try {
				await apiClose(id);
			} catch (e) {
				console.error('close_document error:', e);
			}
			update((panels) => panels.filter((p) => p.doc.id !== id));
		},

		/** Update the body text. */
		updateBody(id: string, body: string) {
			update((panels) =>
				panels.map((p) =>
					p.doc.id === id ? { ...p, doc: { ...p.doc, body }, dirty: true } : p
				)
			);
		},

		/** Update the title. */
		updateTitle(id: string, title: string) {
			update((panels) =>
				panels.map((p) =>
					p.doc.id === id ? { ...p, doc: { ...p.doc, title }, dirty: true } : p
				)
			);
		},

		/** Bring a panel to the front. */
		bringToFront(id: string) {
			const z = nextZ++;
			update((panels) =>
				panels.map((p) => (p.doc.id === id ? { ...p, zIndex: z } : p))
			);
		},

		/** Update panel position (after drag). */
		updatePosition(id: string, x: number, y: number) {
			update((panels) =>
				panels.map((p) =>
					p.doc.id === id ? { ...p, position: { x, y } } : p
				)
			);
		},

		/** Switch panel mode (edit / preview / history). */
		setMode(id: string, mode: 'edit' | 'preview' | 'history') {
			update((panels) =>
				panels.map((p) => (p.doc.id === id ? { ...p, mode } : p))
			);
		},

		/** Lazy-load commits for history mode. */
		async loadCommits(id: string) {
			const current = get({ subscribe });
			const panel = current.find((p) => p.doc.id === id);
			if (!panel || panel.commitsLoaded) return;
			try {
				const commits = await apiListCommits(id);
				update((panels) =>
					panels.map((p) =>
						p.doc.id === id
							? { ...p, commits, commitsLoaded: true }
							: p
					)
				);
			} catch (e) {
				console.error('Failed to load commits:', e);
			}
		},

		/** Restore a document to a specific commit. */
		async restoreVersion(id: string, commitId: string) {
			try {
				const doc = await apiRestoreCommit(id, commitId);
				update((panels) =>
					panels.map((p) =>
						p.doc.id === id
							? { ...p, doc, dirty: false, mode: 'edit', commitsLoaded: false }
							: p
					)
				);
			} catch (e) {
				console.error('Failed to restore commit:', e);
			}
		},

		/** Select a commit in history view. */
		selectCommit(id: string, index: number | null) {
			update((panels) =>
				panels.map((p) =>
					p.doc.id === id ? { ...p, selectedCommit: index } : p
				)
			);
		},

		/** Toggle skills overflow dropdown. */
		toggleSkillsOverflow(id: string) {
			update((panels) =>
				panels.map((p) =>
					p.doc.id === id
						? { ...p, skillsOverflowOpen: !p.skillsOverflowOpen }
						: p
				)
			);
		}
	};
}

export const documents = createDocumentsStore();
