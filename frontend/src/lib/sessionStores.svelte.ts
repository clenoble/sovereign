/**
 * The stores' initial states, for `resetSessionState()` (`$lib/session.ts`;
 * C-AREA AR4, spec v2.0.82, ch. 07: « The interface starts from nothing at a
 * switch. No window, list, chat or notice of the previous session remains
 * in it. »).
 *
 * Every store module under `$lib/stores/` is read here (so a new store is
 * covered without being named), and each state object it exports is
 * snapshotted when this module is first evaluated, before the app has run:
 * that snapshot is the store's initial state. `restoreInitialStates()` puts
 * every state back to it, field by field (the objects stay the same, so
 * every component reading them sees the change).
 *
 * **Kept**, the device's own state and what the login itself sets:
 * - `device` (the viewport and platform) and `theme`, whole;
 * - `app.bubbleStyle` (a device preference read before login, C-AREA AR2)
 *   and `app.authState` (set by the login).
 *
 * Not reached: what a store keeps privately, unexported (timers, maps).
 */

const modules = import.meta.glob('./stores/*.svelte.ts', { eager: true }) as Record<
	string,
	Record<string, unknown>
>;

/** Stores kept whole across a switch. */
const KEPT_STORES = new Set(['device', 'theme']);

/** Fields kept across a switch, by store. */
const KEPT_FIELDS: Record<string, Set<string>> = {
	app: new Set(['bubbleStyle', 'authState'])
};

interface Entry {
	name: string;
	state: unknown;
	initial: unknown;
}

const entries: Entry[] = [];
for (const m of Object.values(modules)) {
	for (const [name, value] of Object.entries(m)) {
		if (typeof value !== 'object' || value === null || KEPT_STORES.has(name)) continue;
		entries.push({ name, state: value, initial: $state.snapshot(value) });
	}
}

/** Puts every store's state back to its initial value, except what is kept. */
export function restoreInitialStates(): void {
	for (const e of entries) {
		const initial = $state.snapshot(e.initial) as unknown;
		if (Array.isArray(e.state)) {
			e.state.splice(0, e.state.length, ...(initial as unknown[]));
			continue;
		}
		const state = e.state as Record<string, unknown>;
		const init = initial as Record<string, unknown>;
		const kept = KEPT_FIELDS[e.name];
		try {
			for (const key of Object.keys(state)) {
				if (!(key in init) && !kept?.has(key)) delete state[key];
			}
			for (const [key, value] of Object.entries(init)) {
				if (!kept?.has(key)) state[key] = value;
			}
		} catch {
			// A frozen object (a constant, not a state) has nothing to restore.
		}
	}
}
