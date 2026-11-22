/** C-SWEEP SW5 on the phone (spec ch. 09, lot LS(a)): the review window
 *  groups by type; each row says its state in words, on the row and in its
 *  checkbox's accessible name (`PII suspected - TBC`, `PII confirmed`,
 *  `PII dismissed`; Céline, 2026-11-17); checkboxes select several records
 *  and one tap dismisses or confirms them at once; no gesture acts on a whole
 *  type (Céline, 2026-11-17).
 *
 *  Written before the code by the test author of lot LS(a); the implementer
 *  does not edit this file. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { ReviewGroupDto } from '$lib/api/commands';

const h = vi.hoisted(() => ({ reviewed: [] as Array<[string, string[]]> }));

vi.mock('$lib/api/commands', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api/commands')>();
	return {
		...actual,
		listPiiReviewGroups: vi.fn(async () => []),
		reviewPiiRecords: vi.fn(async (gesture: string, ids: string[]) => {
			h.reviewed.push([gesture, ids]);
			return { rewritten: [] };
		}),
		getPiiSignal: vi.fn(async () => ({ waiting: 0, status: null }))
	};
});

import { piiReview } from '$lib/stores/piiReview.svelte';
import PiiReviewGroups from './PiiReviewGroups.svelte';

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

beforeEach(() => {
	h.reviewed = [];
	piiReview.groups = GROUPS;
	piiReview.selected = [];
	piiReview.loaded = true;
});

describe('PiiReviewGroups', () => {
	it('groups the records under a heading per type', () => {
		const { getAllByRole } = render(PiiReviewGroups);
		const headings = getAllByRole('heading').map((e) => e.textContent?.trim());
		expect(headings).toEqual(expect.arrayContaining(['IBAN', 'E-mail']));
		const groups = getAllByRole('group');
		expect(groups.length).toBe(2);
		expect(within(groups[0]).getAllByRole('checkbox').length).toBe(2);
		expect(within(groups[1]).getAllByRole('checkbox').length).toBe(1);
	});

	it('says each state in words, on the row and in the checkbox accessible name', () => {
		const { container, getAllByRole } = render(PiiReviewGroups);
		const text = container.textContent ?? '';
		for (const words of ['PII suspected - TBC', 'PII confirmed', 'PII dismissed']) {
			expect(text).toContain(words);
		}
		const names = getAllByRole('checkbox').map((c) => c.getAttribute('aria-label') ?? c.closest('label')?.textContent ?? '');
		expect(names.some((n) => n.includes('PII suspected - TBC'))).toBe(true);
		expect(names.some((n) => n.includes('PII confirmed'))).toBe(true);
		expect(names.some((n) => n.includes('PII dismissed'))).toBe(true);
		expect(text).not.toContain('unreviewed');
	});

	it('selects several records and dismisses them in one gesture', async () => {
		const { getAllByRole, getByRole } = render(PiiReviewGroups);
		const boxes = getAllByRole('checkbox');
		await fireEvent.click(boxes[0]);
		await fireEvent.click(boxes[2]);
		await tick();
		await fireEvent.click(getByRole('button', { name: /dismiss selected/i }));
		await tick();
		expect(h.reviewed).toEqual([['dismiss', ['pii_record:1', 'pii_record:3']]]);
	});

	it('confirms the records selected in one gesture', async () => {
		const { getAllByRole, getByRole } = render(PiiReviewGroups);
		await fireEvent.click(getAllByRole('checkbox')[0]);
		await tick();
		await fireEvent.click(getByRole('button', { name: /confirm selected/i }));
		await tick();
		expect(h.reviewed).toEqual([['confirm', ['pii_record:1']]]);
	});

	it('offers no gesture on a whole type', () => {
		const { getAllByRole, queryAllByRole } = render(PiiReviewGroups);
		const names = queryAllByRole('button').map((b) => (b.getAttribute('aria-label') ?? b.textContent ?? '').toLowerCase());
		for (const n of names) {
			expect(n).not.toMatch(/\ball\b|every|iban|e-mail/);
		}
		for (const g of getAllByRole('group')) {
			expect(within(g).queryAllByRole('checkbox', { name: /all/i })).toEqual([]);
		}
	});
});
