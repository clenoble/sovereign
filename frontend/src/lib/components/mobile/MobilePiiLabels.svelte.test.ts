/** C-SWEEP SW5 and SW6 on the phone (spec ch. 09, lot LS(a)):
 *
 *  - A token is shown as its type's label wherever a document's text is
 *    shown on the phone (Céline, 2026-11-17: « Étiquettes sur téléphone »):
 *    the reader, in its preview, its title and its accessible name; the
 *    canvas's cards (read in the source: they show the title through
 *    `shownTitle`).
 *  - SW6: the taskbar's PII tab says `{n} personal data to review` while
 *    suspected values wait, whether or not the dashboard was opened.
 *
 *  Written before the code by the test author of lot LS(a); the implementer
 *  does not edit this file. Its limit, for the source reads: they read names
 *  in the component's text, not what it renders. */
import { beforeEach, describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import type { OpenPanel } from '$lib/stores/documents.svelte';
import { piiSignal } from '$lib/stores/piiReview.svelte';
import MobileDocReader from './MobileDocReader.svelte';
import MobileTaskbar from './MobileTaskbar.svelte';
import readerSource from './MobileDocReader.svelte?raw';
import canvasSource from './MobileCanvas.svelte?raw';

function panel(mode: OpenPanel['mode']): OpenPanel {
	return {
		doc: {
			id: 'document:a',
			title: 'Bank [pii:pii_record:abc123]',
			body: 'pay to [pii:pii_record:abc123] on Monday',
			images: [],
			videos: [],
			thread_id: 'thread:t',
			is_owned: true,
			created_at: '2026-11-17T00:00:00Z',
			modified_at: '2026-11-17T00:00:00Z',
			revision: 'r1',
			labelled_title: 'Bank [IBAN]',
			labelled_body: 'pay to [IBAN] on Monday'
		},
		dirty: false,
		position: { x: 0, y: 0 },
		size: { width: 400, height: 600 },
		zIndex: 100,
		mode,
		commits: [],
		commitsLoaded: false,
		selectedCommit: null,
		skillsOverflowOpen: false,
		saveError: null,
		saveInfo: null,
		conflict: false
	};
}

beforeEach(() => {
	piiSignal.waiting = 0;
});

describe('SW5: the phone shows a token as its label', () => {
	it('in the reader: the preview, the title and the accessible name', () => {
		const { container } = render(MobileDocReader, { props: { panel: panel('preview') } });
		const text = container.textContent ?? '';
		expect(text).not.toContain('[pii:');
		expect(text).toContain('[IBAN]');
		const named = Array.from(container.querySelectorAll('[aria-label]')).map((e) => e.getAttribute('aria-label') ?? '');
		expect(named.some((n) => n.includes('[pii:'))).toBe(false);
	});

	it('the reader and the canvas show what they show through the labels', () => {
		expect(readerSource).toContain('shownTitle');
		expect(readerSource).toContain('shownText');
		expect(readerSource).not.toMatch(/\{panel\.doc\.title\b/);
		expect(readerSource).not.toMatch(/renderMarkdown\(panel\.doc\.body/);
		expect(canvasSource).toContain('shownTitle');
		expect(canvasSource).not.toMatch(/\{doc\.title\b/);
	});
});

describe('SW6: the taskbar says how many wait', () => {
	it('says the count in words while suspected values wait', () => {
		piiSignal.waiting = 12;
		const { container } = render(MobileTaskbar);
		const said = [
			container.textContent ?? '',
			...Array.from(container.querySelectorAll('[aria-label]')).map((e) => e.getAttribute('aria-label') ?? '')
		].join('\n');
		expect(said).toContain('12 personal data to review');
	});

	it('says nothing of it when none waits', () => {
		piiSignal.waiting = 0;
		const { container } = render(MobileTaskbar);
		const said = [
			container.textContent ?? '',
			...Array.from(container.querySelectorAll('[aria-label]')).map((e) => e.getAttribute('aria-label') ?? '')
		].join('\n');
		expect(said).not.toContain('personal data to review');
	});
});
