import { beforeEach, describe, expect, it } from 'vitest';
import type { FullDocument } from '$lib/api/commands';
import { mockTauriCommand } from '$lib/test/tauri';
import { panels, openById, save, keepMine, loadStored, close, updateBody } from './documents.svelte';

// C-DOC adoption step 2: the panel saves through the document service, based
// on the revision it read. A conflict keeps both versions without asking
// (Céline, 2026-10-12): the text is saved on the current revision, the other
// version stays in History, and the panel says so.

function doc(overrides: Partial<FullDocument> = {}): FullDocument {
	return {
		id: 'document:1',
		title: 'T',
		body: 'stored',
		images: [],
		videos: [],
		thread_id: 'thread:1',
		is_owned: true,
		created_at: '2026-10-11T00:00:00Z',
		modified_at: '2026-10-11T00:00:00Z',
		revision: 'r1',
		...overrides
	};
}

interface SaveArgs {
	id: string;
	body: string;
	baseRevision: string;
	explicit: boolean;
}

beforeEach(() => {
	panels.splice(0, panels.length);
});

async function opened(d: FullDocument = doc()) {
	mockTauriCommand('get_document', () => d);
	await openById(d.id);
	return panels[0];
}

describe('save', () => {
	it('sends the revision it read and takes the stored text and revision back', async () => {
		const panel = await opened();
		const calls: SaveArgs[] = [];
		mockTauriCommand<SaveArgs, FullDocument>('save_document', (a) => {
			calls.push(a);
			return doc({ body: 'write to [pii:pii_record:x]', revision: 'r2' });
		});
		updateBody(panel.doc.id, 'write to alice@example.com');
		await save(panel.doc.id, true);
		expect(calls).toHaveLength(1);
		expect(calls[0].baseRevision).toBe('r1');
		expect(calls[0].explicit).toBe(true);
		expect(panels[0].doc.revision).toBe('r2');
		expect(panels[0].doc.body).toBe('write to [pii:pii_record:x]');
		expect(panels[0].dirty).toBe(false);
	});

	it('a conflict saves the text on the current revision and says the other version is kept', async () => {
		const panel = await opened();
		const calls: SaveArgs[] = [];
		mockTauriCommand<SaveArgs, FullDocument>('save_document', (a) => {
			calls.push(a);
			if (a.baseRevision === 'r1') throw 'conflict: This document was changed elsewhere.';
			return doc({ body: 'mine', revision: 'r3' });
		});
		mockTauriCommand('get_document', () => doc({ body: 'theirs', revision: 'r2' }));
		updateBody(panel.doc.id, 'mine');
		await save(panel.doc.id);
		expect(calls.map((c) => c.baseRevision)).toEqual(['r1', 'r2']);
		expect(calls[1].body).toBe('mine');
		expect(calls[1].explicit).toBe(true);
		expect(panels[0].conflict).toBe(false);
		expect(panels[0].saveError).toBeNull();
		expect(panels[0].saveInfo).toContain('other version is in History');
		expect(panels[0].doc.body).toBe('mine');
		expect(panels[0].doc.revision).toBe('r3');
	});

	it('if saving over the other version conflicts again, autosave stops until the user chooses', async () => {
		const panel = await opened();
		let calls = 0;
		mockTauriCommand('save_document', () => {
			calls++;
			throw 'conflict: This document was changed elsewhere.';
		});
		mockTauriCommand('get_document', () => doc({ revision: 'r2' }));
		updateBody(panel.doc.id, 'mine');
		await save(panel.doc.id);
		expect(panels[0].conflict).toBe(true);
		expect(panels[0].saveError).toBe('This document was changed elsewhere.');
		expect(panels[0].doc.body).toBe('mine');
		expect(panels[0].dirty).toBe(true);
		await save(panel.doc.id); // an autosave tick
		expect(calls).toBe(2);
	});

	it('keep mine saves the text on the current revision', async () => {
		const panel = await opened();
		mockTauriCommand('save_document', () => {
			throw 'conflict: changed';
		});
		updateBody(panel.doc.id, 'mine');
		await save(panel.doc.id);
		mockTauriCommand('get_document', () => doc({ body: 'theirs', revision: 'r9' }));
		const calls: SaveArgs[] = [];
		mockTauriCommand<SaveArgs, FullDocument>('save_document', (a) => {
			calls.push(a);
			return doc({ body: a.body, revision: 'r10' });
		});
		await keepMine(panel.doc.id);
		expect(calls[0].baseRevision).toBe('r9');
		expect(calls[0].body).toBe('mine');
		expect(panels[0].conflict).toBe(false);
		expect(panels[0].doc.body).toBe('mine');
	});

	it('load the stored version drops the text', async () => {
		const panel = await opened();
		mockTauriCommand('save_document', () => {
			throw 'conflict: changed';
		});
		updateBody(panel.doc.id, 'mine');
		await save(panel.doc.id);
		mockTauriCommand('get_document', () => doc({ body: 'theirs', revision: 'r9' }));
		await loadStored(panel.doc.id);
		expect(panels[0].doc.body).toBe('theirs');
		expect(panels[0].dirty).toBe(false);
		expect(panels[0].saveError).toBeNull();
	});

	it('closing over a failed save keeps the panel open', async () => {
		const panel = await opened();
		mockTauriCommand('save_document', () => {
			throw 'This document has 2 blocks of text; nothing was changed.';
		});
		let closed = 0;
		mockTauriCommand('close_document', () => {
			closed++;
		});
		updateBody(panel.doc.id, 'mine');
		await close(panel.doc.id);
		expect(panels).toHaveLength(1);
		expect(closed).toBe(0);
		expect(panels[0].saveError).toContain('2 blocks');
	});
});
