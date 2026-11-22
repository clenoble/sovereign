/** v0.0.13 lot L5, B1 — C-MERGE on the phone (spec ch. 03, *Merging keeps the
 *  source thread — contract (C-MERGE)*, v2.0.109).
 *
 *  - MG2: « On the phone, which shows one lane at a time, the ended lane shows
 *    its mark », « merged into '{target}' on {YYYY-MM-DD} », the day in the
 *    device's local time.
 *  - MG3: « No picker, menu, drop or assistant offers it for a document »: the
 *    card menu's « Move to Thread », the share sheet's thread list, the trash's
 *    « Restore into… » chooser, the alerts' « Move to a thread… » chooser, and
 *    the « + » on the ended lane.
 *  - The « + » is not shown on an ended lane (the Fable maille, by delegation of
 *    Céline, 2026-11-17; b1-tests-v2). In v1 the test held only that no
 *    document lands in the ended thread.
 *
 *  The ended thread reaches the phone as `canvas_load` gives it: a `ThreadDto`
 *  with `merged_into`, `merged_into_name` and `ended_at` (the « types and
 *  signatures » commit; the Rust side is `tauri_commands::canvas::thread_dtos`,
 *  tested in `crates/sovereign-app/src/b1_merge_tests.rs`).
 *
 *  Written before the code by the B1 test author; the implementer does not
 *  edit this file. A test that looks wrong is reported, not worked around. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { ThreadDto } from '$lib/api/commands';
import { mockTauriCommand } from '$lib/test/tauri';

const h = vi.hoisted(() => ({
	created: [] as Array<[string, string]>
}));

vi.mock('$lib/api/commands', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api/commands')>();
	return {
		...actual,
		createDocument: vi.fn(async (title: string, threadId: string) => {
			h.created.push([title, threadId]);
			return 'document:new';
		})
	};
});

import LaneHeader from './LaneHeader.svelte';
import Fab from './Fab.svelte';
import SharePickerSheet from './SharePickerSheet.svelte';
import ContextMenu from '../ContextMenu.svelte';
import TrashPanel from '../TrashPanel.svelte';
import SyncAlerts from '../SyncAlerts.svelte';
import { canvas, mobileCanvas, setLaneIndex } from '$lib/stores/canvas.svelte';
import { app } from '$lib/stores/app.svelte';
import { trash } from '$lib/stores/trash.svelte';
import { syncAlerts, reloadSyncAlerts } from '$lib/stores/syncAlerts.svelte';

/** « merged into '{target}' on {YYYY-MM-DD} » */
const MARK = "merged into '{target}' on {date}";

const ENDED_AT = '2026-11-17T21:30:00.000Z';

/** The day of `iso` in the device's local time, as YYYY-MM-DD. */
function localDay(iso: string): string {
	const d = new Date(iso);
	const p = (n: number) => String(n).padStart(2, '0');
	return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

const GARDEN: ThreadDto = {
	id: 'thread:g',
	name: 'Garden',
	description: '',
	created_at: '2026-01-01T00:00:00Z',
	merged_into: 'thread:k',
	merged_into_name: 'Kitchen',
	ended_at: ENDED_AT
};
const KITCHEN: ThreadDto = { id: 'thread:k', name: 'Kitchen', description: '', created_at: '2026-01-02T00:00:00Z' };
const SHED: ThreadDto = { id: 'thread:s', name: 'Shed', description: '', created_at: '2026-01-03T00:00:00Z' };
const THREADS = [GARDEN, KITCHEN, SHED];

beforeEach(() => {
	h.created.length = 0;
	canvas.threads = THREADS;
	canvas.documents = [];
	mobileCanvas.currentLaneIndex = 0;
	mobileCanvas.currentLaneId = null;
	app.contextMenu = null;
	app.pendingShare = null;
	window.prompt = vi.fn(() => 'Minutes');
	window.alert = vi.fn();
	mockTauriCommand('list_threads', () => THREADS);
	mockTauriCommand('canvas_load', () => ({ documents: [], threads: THREADS, relationships: [], milestones: [] }));
});

describe('MG2 on the phone: the ended lane shows its mark', () => {
	it('the lane header names the ended lane and shows « merged into … on … »', async () => {
		setLaneIndex(0);
		const r = render(LaneHeader);
		await tick();
		const text = r.container.textContent ?? '';
		expect(text).toContain('Garden');
		expect(text).toContain(MARK.replace('{target}', 'Kitchen').replace('{date}', localDay(ENDED_AT)));
	});

	it('a live lane shows no mark', async () => {
		setLaneIndex(1);
		const r = render(LaneHeader);
		await tick();
		expect(r.container.textContent ?? '').not.toContain('merged into');
	});
});

describe('MG3 on the phone: an ended thread is offered for nothing', () => {
	it('the card menu’s « Move to Thread » does not list it', async () => {
		app.contextMenu = { x: 10, y: 10, docId: 'document:p', threadId: SHED.id };
		const r = render(ContextMenu);
		await fireEvent.click(r.getByText(/Move to Thread/));
		await tick();
		const offered = Array.from(r.container.querySelectorAll('.sub-menu button')).map((b) => b.textContent?.trim());
		expect(offered).toEqual(['Kitchen']);
	});

	it('the share sheet does not list it', async () => {
		app.pendingShare = { contentType: 'text', text: 'hello' };
		const r = render(SharePickerSheet);
		await tick();
		const offered = Array.from(r.container.querySelectorAll('button[aria-label^="Save to "]')).map((b) =>
			b.getAttribute('aria-label')
		);
		expect(offered).toEqual(['Save to Kitchen', 'Save to Shed']);
	});

	it('the trash’s « Restore into… » chooser does not list it', async () => {
		trash.lines = [];
		trash.emptyRequested = false;
		trash.error = null;
		mockTauriCommand('list_trash', () => [
			{
				id: 'document:b',
				title: 'Beta',
				thread_id: 'thread:gone',
				thread_name: null,
				deleted_thread_name: 'Old notes',
				trashed_at: '2026-10-20T10:00:00Z',
				days_left: 26
			}
		]);
		const r = render(TrashPanel);
		await fireEvent.click(await r.findByRole('button', { name: 'Restore Beta' }));
		const dialog = await r.findByRole('dialog');
		const select = within(dialog).getByRole('combobox', { name: /thread/i }) as HTMLSelectElement;
		expect(Array.from(select.options).map((o) => o.textContent?.trim())).toEqual(['Kitchen', 'Shed']);
	});

	it('the alerts’ « Move to a thread… » chooser does not list it', async () => {
		syncAlerts.alerts = [];
		syncAlerts.previousValues = [];
		syncAlerts.olderDevices = [];
		syncAlerts.orphans = [];
		syncAlerts.open = false;
		mockTauriCommand('list_previous_values', () => []);
		mockTauriCommand('list_sync_alerts', () => ({ alerts: [], could_not_check: null }));
		mockTauriCommand('list_orphaned_documents', () => ({
			orphans: [
				{
					doc_id: 'document:a',
					title: 'Alpha',
					thread_id: 'thread:gone',
					deleted_thread_name: 'Old drafts',
					device: 'peer-a',
					device_name: 'Laptop'
				}
			],
			could_not_check: null
		}));
		const r = render(SyncAlerts, { props: { variant: 'mobile' } });
		await reloadSyncAlerts();
		await tick();
		const opener = r.container.querySelector('.notices [aria-haspopup="dialog"]') as HTMLElement | null;
		expect(opener, 'a notice button opens the sheet').not.toBeNull();
		await fireEvent.click(opener!);
		await tick();
		const dialog = r.container.querySelector('[role="dialog"]') as HTMLElement;
		await fireEvent.click(within(dialog).getAllByRole('button', { name: /move to a thread/i })[0]);
		await tick();
		const select = dialog.querySelector('select') as HTMLSelectElement;
		expect(select, 'the thread chooser').not.toBeNull();
		expect(Array.from(select.options).map((o) => o.textContent?.trim())).toEqual(['Kitchen', 'Shed']);
	});

	it('the « + » is not shown on the ended lane, and is on a live lane', async () => {
		setLaneIndex(0);
		const ended = render(Fab);
		await tick();
		expect(ended.container.querySelector('button.fab'), 'no « + » on the ended lane').toBeNull();
		expect(h.created).toEqual([]);
		ended.unmount();

		setLaneIndex(1);
		const live = render(Fab);
		await tick();
		expect(live.container.querySelector('button.fab'), 'a live lane has its « + »').not.toBeNull();
	});
});
