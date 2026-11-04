/**
 * Test helpers for live regions (v0.0.12 F-a11y, F2,
 * `doc/plans/V0012-frontend-a11y.md`).
 *
 * An element "announces" when a screen reader reads the changes made inside
 * it: an explicit `aria-live` of `polite` or `assertive`, or a role whose
 * implicit live value is not `off` (`log`, `status`: polite; `alert`:
 * assertive), unless it carries `aria-live="off"`. The helpers do not judge
 * which of these an implementation picks: the contract allows the log itself
 * (live) or a separate `aria-live="polite"` region.
 */

const LIVE_SELECTOR = '[aria-live], [role="log"], [role="status"], [role="alert"]';

/** The elements under `root` whose changes are read out. */
export function announcers(root: ParentNode): HTMLElement[] {
	return (Array.from(root.querySelectorAll(LIVE_SELECTOR)) as HTMLElement[]).filter(
		(el) => (el.getAttribute('aria-live') ?? '').trim().toLowerCase() !== 'off'
	);
}

/** The text a screen reader could read from `regions` (still in the page). */
export function announcedText(regions: HTMLElement[]): string {
	return regions
		.filter((el) => el.isConnected)
		.map((el) => el.textContent ?? '')
		.join('\n');
}

/** Lets timers an implementation may use (clear, then set) run. */
export function settle(ms = 60): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, ms));
}
