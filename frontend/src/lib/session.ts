import { restoreInitialStates } from './sessionStores.svelte';

/**
 * C-AREA AR4 (spec v2.0.82, ch. 07): « The interface starts from nothing at
 * a switch. No window, list, chat or notice of the previous session remains
 * in it. »
 *
 * What a login runs before the new session is shown: every store under
 * `$lib/stores/` goes back to its initial state, except the device's own
 * state (the viewport, the theme and the bubble style read before login)
 * and what the login itself sets (`app.authState`).
 *
 * Its signature is the test author's (claude-coord from-windows/0249); the
 * login (`LoginScreen.svelte`) calls it. Its test is
 * `$lib/stores/c_area_reset.svelte.test.ts`; the initial states are kept in
 * `$lib/sessionStores.svelte.ts`.
 */
export function resetSessionState(): void {
	restoreInitialStates();
}
