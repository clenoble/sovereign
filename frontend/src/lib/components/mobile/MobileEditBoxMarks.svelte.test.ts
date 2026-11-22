/** C-SWEEP SW5 on the phone, the edit box (spec v2.0.114, ch. 09, SW5, its last
 *  bullet; Céline, 2026-11-17: « oui, 1 pour la v0.0.13 »; the words and the
 *  screen reader's part by the Fable maille, by delegation, 2026-11-17):
 *
 *  - The box keeps the stored form `[pii:…]`. When the body holds a token, and
 *    only then, one line above the box says: « In this box, personal data
 *    shows as marks like [pii:…]. Leave a mark as it is to keep its value. »
 *  - It is the box's description for a screen reader (`aria-describedby`), not
 *    a live region.
 *
 *  Written before the code by the test author of lot LS(a) (ls-a-tests-v4);
 *  the implementer does not edit this file. Red today: the line does not exist. */
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import type { OpenPanel } from '$lib/stores/documents.svelte';
import MobileDocReader from './MobileDocReader.svelte';

/** The line, compared whole. */
const LINE = 'In this box, personal data shows as marks like [pii:…]. Leave a mark as it is to keep its value.';

const WITH_TOKEN = 'pay to [pii:pii_record:abc123] on Monday';
const WITHOUT_TOKEN = 'pay on Monday';

function panel(body: string): OpenPanel {
	return {
		doc: {
			id: 'document:a',
			title: 'Bank',
			body,
			images: [],
			videos: [],
			thread_id: 'thread:t',
			is_owned: true,
			created_at: '2026-11-17T00:00:00Z',
			modified_at: '2026-11-17T00:00:00Z',
			revision: 'r1',
			labelled_title: 'Bank',
			labelled_body: body.replace('[pii:pii_record:abc123]', '[IBAN]')
		},
		dirty: false,
		position: { x: 0, y: 0 },
		size: { width: 400, height: 600 },
		zIndex: 100,
		mode: 'edit',
		commits: [],
		commitsLoaded: false,
		selectedCommit: null,
		skillsOverflowOpen: false,
		saveError: null,
		saveInfo: null,
		conflict: false
	} as OpenPanel;
}

/** The elements whose own text is the line. */
function lines(container: HTMLElement): HTMLElement[] {
	return (Array.from(container.querySelectorAll('*')) as HTMLElement[]).filter(
		(e) => (e.textContent ?? '').trim() === LINE && !Array.from(e.children).some((c) => (c.textContent ?? '').trim() === LINE)
	);
}

describe('SW5: the line above the phone’s edit box', () => {
	it('shows when the body holds a token, and not without one; the box keeps the stored form', () => {
		const withToken = render(MobileDocReader, { props: { panel: panel(WITH_TOKEN) } });
		const box = withToken.container.querySelector('textarea') as HTMLTextAreaElement | null;
		expect(box, 'the edit box').not.toBeNull();
		expect(box!.value, 'the box holds the stored form').toBe(WITH_TOKEN);
		expect(lines(withToken.container).length, 'one line, its words whole').toBe(1);
		withToken.unmount();

		const without = render(MobileDocReader, { props: { panel: panel(WITHOUT_TOKEN) } });
		expect(without.container.querySelector('textarea'), 'the edit box').not.toBeNull();
		expect(without.container.textContent ?? '', 'no line without a token').not.toContain('personal data shows as marks');
	});

	it('is the box’s description (aria-describedby), not a live region', () => {
		const { container } = render(MobileDocReader, { props: { panel: panel(WITH_TOKEN) } });
		const box = container.querySelector('textarea') as HTMLTextAreaElement;
		const ids = (box.getAttribute('aria-describedby') ?? '').split(/\s+/).filter(Boolean);
		expect(ids.length, 'the box has a description').toBeGreaterThan(0);
		const described = ids.map((id) => document.getElementById(id)).filter(Boolean) as HTMLElement[];
		const line = described.find((e) => (e.textContent ?? '').trim() === LINE);
		expect(line, 'aria-describedby names the line’s element').toBeTruthy();
		for (let e: HTMLElement | null = line!; e && e !== container; e = e.parentElement) {
			expect(e.hasAttribute('aria-live'), 'not a live region (aria-live)').toBe(false);
			expect(['status', 'alert', 'log'], 'not a live region (role)').not.toContain(e.getAttribute('role') ?? '');
		}
	});
});
