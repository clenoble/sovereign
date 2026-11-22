/** v0.0.12 F-a11y, F1 — each notice button names its object (WCAG 2.4.6),
 *  and its name begins with its visible text (WCAG 2.5.3), desktop and mobile (`doc/plans/V0012-frontend-a11y.md`, *F1*). Written
 *  before the code, by another author: whoever implements F1 makes these pass
 *  without editing this file.
 *
 *  What the code author provides, in `SyncAlerts.svelte`:
 *  - the alerts notice's button: accessible name "See the alert" (one) /
 *    "See the alerts" (several), as today;
 *  - the orphans notice's button: accessible name "See it: the document
 *    waiting for a decision" (one) / "See them: the documents waiting for a
 *    decision" (several);
 *  - the unreadable notice's button: accessible name "See it: the document
 *    that could not be read" (one) / "See them: the documents that could not
 *    be read" (several);
 *  - **[choice of the contract]** the visible text of the last two stays
 *    "See it" / "See them", the name coming from `aria-label`; every notice
 *    button keeps `aria-haspopup="dialog"`;
 *  - every notice button's accessible name begins with its visible text
 *    (2.5.3, Label in Name: a person using speech input says what they see).
 *    Contract amended to this on 2026-10-25 (`7dddd2f`).
 *
 *  The names are matched exactly (`getByRole('button', { name })`, exact
 *  string). The store is driven directly (`syncAlerts.*`), as a reload would
 *  leave it; the commands a click could reach are mocked all the same.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { render, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { canvas } from '$lib/stores/canvas.svelte';
import SyncAlerts from './SyncAlerts.svelte';
import { syncAlerts, type Orphan, type SyncAlert } from '$lib/stores/syncAlerts.svelte';

const alert = (id: string): SyncAlert => ({
	docId: `document:${id}`,
	title: `Notes ${id}`,
	device: 'Laptop',
	finding: 'injected instructions in the new content',
	previousVersion: null
});

const orphan = (id: string): Orphan => ({
	docId: `document:${id}`,
	title: `Doc ${id}`,
	threadId: 'thread:gone',
	oldThreadName: 'Old drafts',
	device: 'Laptop'
});

const unreadable = (n: number) => ({
	heading: 'Documents that could not be read',
	meaning: 'Each one is kept as it was, untouched. Sovereign will try to read it again after its next update.',
	lines: Array.from({ length: n }, (_, i) => `Document 0000000${i} — last modified 2025-03-02 12:34`)
});

beforeEach(() => {
	syncAlerts.alerts = [];
	syncAlerts.previousValues = [];
	syncAlerts.olderDevices = [];
	syncAlerts.orphans = [];
	syncAlerts.unreadable = { heading: '', meaning: '', lines: [] };
	syncAlerts.open = false;
	canvas.threads = [];
	mockTauriCommand('list_threads', () => []);
	mockTauriCommand('canvas_load', () => ({ documents: [], threads: [], relationships: [], milestones: [] }));
	mockTauriCommand('list_sync_alerts', () => ({ alerts: [], could_not_check: null }));
	mockTauriCommand('list_previous_values', () => []);
	mockTauriCommand('list_orphaned_documents', () => ({ orphans: [], could_not_check: null }));
	mockTauriCommand('list_unreadable_documents', () => unreadable(0));
});

/** The notices' area (the dialog, closed here, is not in it). */
function notices(container: HTMLElement): HTMLElement {
	const n = container.querySelector('.notices') as HTMLElement | null;
	expect(n, 'the notices are shown').not.toBeNull();
	return n!;
}

const cases = [
	{
		notice: 'alerts',
		set: (n: number) => {
			syncAlerts.alerts = Array.from({ length: n }, (_, i) => alert(String.fromCharCode(97 + i)));
		},
		one: { name: 'See the alert', text: 'See the alert' },
		many: { name: 'See the alerts', text: 'See the alerts' }
	},
	{
		notice: 'orphans',
		set: (n: number) => {
			syncAlerts.orphans = Array.from({ length: n }, (_, i) => orphan(String.fromCharCode(97 + i)));
		},
		one: { name: 'See it: the document waiting for a decision', text: 'See it' },
		many: { name: 'See them: the documents waiting for a decision', text: 'See them' }
	},
	{
		notice: 'unreadable',
		set: (n: number) => {
			syncAlerts.unreadable = unreadable(n);
		},
		one: { name: 'See it: the document that could not be read', text: 'See it' },
		many: { name: 'See them: the documents that could not be read', text: 'See them' }
	}
] as const;

describe.each(['desktop', 'mobile'] as const)('SyncAlerts (%s) — F1, each notice button names its object', (variant) => {
	for (const c of cases) {
		for (const [count, label, want] of [
			[1, 'one', c.one],
			[3, 'several', c.many]
		] as const) {
			it(`${c.notice}, ${label}: the button is named "${want.name}", reads "${want.text}", opens a dialog`, async () => {
				c.set(count);
				const { container } = render(SyncAlerts, { props: { variant } });
				await tick();
				// Found by its exact accessible name: that name is `want.name`.
				const button = within(notices(container)).getByRole('button', { name: want.name });
				const visible = button.textContent?.trim() ?? '';
				expect(visible, 'the visible text is unchanged').toBe(want.text);
				expect(
					want.name.startsWith(visible),
					`2.5.3: the accessible name "${want.name}" begins with the visible text "${visible}"`
				).toBe(true);
				expect(button.getAttribute('aria-haspopup')).toBe('dialog');
			});
		}
	}

	it('with all three notices shown, the three buttons have three different names', async () => {
		cases.forEach((c) => c.set(1));
		const { container } = render(SyncAlerts, { props: { variant } });
		await tick();
		const area = within(notices(container));
		const named = cases.map((c) => area.getByRole('button', { name: c.one.name }));
		expect(new Set(named).size, 'three distinct buttons').toBe(3);
		const openers = Array.from(notices(container).querySelectorAll('[aria-haspopup="dialog"]'));
		expect(openers.length, 'one opener per notice').toBe(3);
	});

	it('the names follow the count when it changes after render (one → several)', async () => {
		cases.forEach((c) => c.set(1));
		const { container } = render(SyncAlerts, { props: { variant } });
		await tick();
		cases.forEach((c) => c.set(2));
		await tick();
		const area = within(notices(container));
		for (const c of cases) {
			const b = area.getByRole('button', { name: c.many.name });
			const visible = b.textContent?.trim() ?? '';
			expect(visible).toBe(c.many.text);
			expect(c.many.name.startsWith(visible), `2.5.3: "${c.many.name}" begins with "${visible}"`).toBe(true);
		}
	});
});
