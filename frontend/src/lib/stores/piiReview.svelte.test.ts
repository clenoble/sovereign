/** C-SWEEP on the phone (spec ch. 09, v2.0.111-112, lot LS(a)): the review
 *  window's store (SW5) and the signal (SW6).
 *
 *  - SW5: the window groups by type; checkboxes select one record or several,
 *    and the gestures act on the selection, never on a whole type (Céline,
 *    2026-11-17).
 *  - SW6: `{n} personal data to review`; the notice at login, in the Fable
 *    maille's words, yields to every other notice due at that login.
 *
 *  Written before the code by the test author of lot LS(a); the implementer
 *  does not edit this file. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReviewGroupDto } from '$lib/api/commands';

const h = vi.hoisted(() => ({
	reviewed: [] as Array<[string, string[]]>,
	groups: [] as unknown[],
	notice: null as { title: string; body: string } | null,
	signal: { waiting: 0, status: null as string | null }
}));

vi.mock('$lib/api/commands', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api/commands')>();
	return {
		...actual,
		listPiiReviewGroups: vi.fn(async () => h.groups),
		reviewPiiRecords: vi.fn(async (gesture: string, ids: string[]) => {
			h.reviewed.push([gesture, ids]);
			return { rewritten: [] };
		}),
		getPiiSignal: vi.fn(async () => h.signal),
		takePiiLoginNotice: vi.fn(async () => h.notice)
	};
});

import {
	confirmSelected,
	dismissPiiLoginNotice,
	dismissSelected,
	loadPiiSignal,
	loadReviewGroups,
	offerPiiLoginNotice,
	piiLoginNotice,
	piiReview,
	piiSignal,
	piiStatusWords,
	takeBackConfirmationSelected,
	takeBackDismissalSelected,
	toggleSelected
} from './piiReview.svelte';
import { setupNotice } from './setupNotice.svelte';
import { migrationNotice } from './migrationNotice.svelte';
import { syncStopped } from './syncStopped.svelte';

const GROUPS: ReviewGroupDto[] = [
	{
		kind: 'iban',
		label: 'IBAN',
		rows: [
			{ record_id: 'pii_record:1', state: 'unreviewed', state_words: 'PII suspected - TBC', documents: ['document:a'] },
			{ record_id: 'pii_record:2', state: 'confirmed', state_words: 'PII confirmed', documents: ['document:b'] }
		]
	},
	{
		kind: 'email',
		label: 'E-mail',
		rows: [{ record_id: 'pii_record:3', state: 'dismissed', state_words: 'PII dismissed', documents: ['document:a'] }]
	}
];

const NOTICE = {
	title: 'Personal data to review',
	body: '3 more since your last login, 12 in all. They wait in the PII dashboard.'
};

beforeEach(() => {
	h.reviewed = [];
	h.groups = GROUPS;
	h.notice = null;
	h.signal = { waiting: 0, status: null };
	piiReview.groups = [];
	piiReview.selected = [];
	piiReview.loaded = false;
	piiSignal.waiting = 0;
	piiLoginNotice.title = null;
	piiLoginNotice.body = null;
	setupNotice.visible = false;
	migrationNotice.text = null;
	syncStopped.noticeShown = false;
});

describe('SW5: the review window', () => {
	it('loads the groups by type, with the state words', async () => {
		await loadReviewGroups();
		expect(piiReview.loaded).toBe(true);
		expect(piiReview.groups.map((g) => g.kind)).toEqual(['iban', 'email']);
		expect(piiReview.groups[0].rows[0].state_words).toBe('PII suspected - TBC');
	});

	it('selects several records with their checkboxes and dismisses them at once', async () => {
		await loadReviewGroups();
		toggleSelected('pii_record:1');
		toggleSelected('pii_record:3');
		await dismissSelected();
		expect(h.reviewed).toEqual([['dismiss', ['pii_record:1', 'pii_record:3']]]);
		expect(piiReview.selected).toEqual([]);
	});

	it('confirms the selection, and a second tap on a checkbox unselects', async () => {
		await loadReviewGroups();
		toggleSelected('pii_record:1');
		toggleSelected('pii_record:2');
		toggleSelected('pii_record:2');
		await confirmSelected();
		expect(h.reviewed).toEqual([['confirm', ['pii_record:1']]]);
	});

	it('takes back a dismissal and a confirmation, on the selection', async () => {
		await loadReviewGroups();
		toggleSelected('pii_record:3');
		await takeBackDismissalSelected();
		toggleSelected('pii_record:2');
		await takeBackConfirmationSelected();
		expect(h.reviewed).toEqual([
			['take_back_dismissal', ['pii_record:3']],
			['take_back_confirmation', ['pii_record:2']]
		]);
	});

	it('acts on nothing when nothing is selected, and never on a whole type', async () => {
		await loadReviewGroups();
		await dismissSelected();
		await confirmSelected();
		expect(h.reviewed).toEqual([]);
		for (const [, ids] of h.reviewed) {
			expect(ids.every((id) => id.startsWith('pii_record:'))).toBe(true);
		}
	});
});

describe('SW6: the signal', () => {
	it('says how many wait, and nothing when none waits', async () => {
		h.signal = { waiting: 12, status: '12 personal data to review' };
		await loadPiiSignal();
		expect(piiSignal.waiting).toBe(12);
		expect(piiStatusWords()).toBe('12 personal data to review');
		piiSignal.waiting = 0;
		expect(piiStatusWords()).toBeNull();
	});

	it('shows the notice at login when the queue has grown', async () => {
		h.notice = NOTICE;
		await offerPiiLoginNotice();
		expect(piiLoginNotice.title).toBe(NOTICE.title);
		expect(piiLoginNotice.body).toBe(NOTICE.body);
		dismissPiiLoginNotice();
		expect(piiLoginNotice.title).toBeNull();
	});

	it('shows nothing when the queue has not grown', async () => {
		h.notice = null;
		await offerPiiLoginNotice();
		expect(piiLoginNotice.title).toBeNull();
	});

	it.each([
		['the setup notice', () => (setupNotice.visible = true)],
		['the migration notice', () => (migrationNotice.text = 'converted')],
		['the sync-stopped notice', () => (syncStopped.noticeShown = true)]
	])('yields to %s', async (_name, other) => {
		h.notice = NOTICE;
		other();
		await offerPiiLoginNotice();
		expect(piiLoginNotice.title).toBeNull();
	});
});
