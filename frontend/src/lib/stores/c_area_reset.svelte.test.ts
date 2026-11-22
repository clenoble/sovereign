/**
 * C-AREA AR4 (spec v2.0.82, ch. 07): « The interface starts from nothing at
 * a switch. No window, list, chat or notice of the previous session remains
 * in it. » **The test, in the interface's own suite: after a login, every
 * store is in its initial state.**
 *
 * Written before the code, by another author than whoever builds the reset:
 * the implementer does not edit this file; a test that looks wrong is
 * reported, not worked around.
 *
 * - **Every store is enumerated** (`import.meta.glob` over this folder) and
 *   must be listed in `STORES`, with its state exports: a new store, or a new
 *   exported object in a store, fails until it is listed (the ratchet form).
 * - **Each listed state is filled** with something a previous session could
 *   have left (every field changed, every list given an item), then **what a
 *   login does is run**: `resetSessionState()` (`$lib/session.ts`, its
 *   signature by the test author), which the login must call; and every
 *   state must be back to its initial value.
 * - **Kept across a switch, with their reason** (`KEPT`): the device's own
 *   state, not a session's — the viewport and platform (`device`), the theme
 *   and the bubble style read before login (the spec's device preferences),
 *   and `app.authState`, which the login itself sets.
 * - **The login calls it:** `LoginScreen.svelte`'s login calls
 *   `resetSessionState()` before it shows the unlocked app.
 *
 * Not covered: module-level state a store does not export (timers, private
 * maps); it cannot be reached from here.
 */
import { isDeepStrictEqual } from 'node:util';
import { describe, expect, it } from 'vitest';
import { resetSessionState } from '$lib/session';
import loginScreen from '$lib/components/LoginScreen.svelte?raw';

/** Every store module, by file name, with its exported state objects. */
const STORES: Record<string, string[]> = {
	'app.svelte.ts': ['app'],
	'browser.svelte.ts': ['browser'],
	'canvas.svelte.ts': ['canvas', 'mobileCanvas'],
	'chat.svelte.ts': ['chat'],
	'contacts.svelte.ts': ['contactsState'],
	'deleteUndo.svelte.ts': ['undo'],
	'device.svelte.ts': ['device'],
	'documents.svelte.ts': ['panels'],
	'migrationNotice.svelte.ts': ['migrationNotice'],
	'pairing.svelte.ts': ['pairing'],
	'pii.svelte.ts': ['piiState'],
	// C-SWEEP (lot LS(a), added by its test author): the review window, the
	// queue's signal and the notice at login are the session's (SW4).
	'piiReview.svelte.ts': ['piiReview', 'piiSignal', 'piiLoginNotice'],
	'recovery.svelte.ts': ['recovery'],
	'recoverySetup.svelte.ts': ['recoverySetup'],
	'setupNotice.svelte.ts': ['setupNotice'],
	'suggestions.svelte.ts': ['suggestions'],
	'sync.svelte.ts': ['sync'],
	'syncAlerts.svelte.ts': ['syncAlerts'],
	'syncStopped.svelte.ts': ['syncStopped'],
	'theme.svelte.ts': ['theme'],
	'trash.svelte.ts': ['trash'],
	'vision.svelte.ts': ['vision'],
	'voice.svelte.ts': ['voice']
};

/** What a switch keeps, with its reason: `store.field`, or `store` whole. */
const KEPT: Record<string, string> = {
	device: "the device's viewport and platform, not a session's",
	theme: 'a device preference read before login (spec ch. 07, C-AREA AR2: « may change », shared)',
	'app.bubbleStyle': 'a device preference read before login (as above)',
	'app.authState': 'set by the login itself'
};

const modules = import.meta.glob('./*.svelte.ts', { eager: true }) as Record<string, Record<string, unknown>>;

function fileOf(path: string): string {
	return path.replace(/^\.\//, '');
}

/** A value a previous session could have left in place of `v`. */
function stale(v: unknown): unknown {
	if (typeof v === 'boolean') return !v;
	if (typeof v === 'number') return v + 1;
	if (typeof v === 'string') return v + ' (left by the previous session)';
	if (v === null || v === undefined) return { leftByThePreviousSession: true };
	if (Array.isArray(v)) return [...v, { leftByThePreviousSession: true }];
	if (v instanceof Set) return new Set([...v, 'left by the previous session']);
	if (v instanceof Map) return new Map([...v, ['left by the previous session', true]]);
	if (typeof v === 'object') return { ...(v as object), leftByThePreviousSession: true };
	return v;
}

function fill(name: string, state: unknown) {
	if (Array.isArray(state)) {
		state.push({ leftByThePreviousSession: true });
		return;
	}
	const obj = state as Record<string, unknown>;
	for (const key of Object.keys(obj)) {
		if (KEPT[`${name}.${key}`]) continue;
		obj[key] = stale(obj[key]);
	}
}

/** The comparable value of a state: kept fields left out. */
function comparable(name: string, state: unknown): unknown {
	const snap = $state.snapshot(state) as unknown;
	if (Array.isArray(snap) || snap === null || typeof snap !== 'object') return snap;
	const out: Record<string, unknown> = {};
	for (const [k, v] of Object.entries(snap as Record<string, unknown>)) {
		if (!KEPT[`${name}.${k}`]) out[k] = v;
	}
	return out;
}

describe('C-AREA AR4: the interface starts from nothing at a switch', () => {
	it('every store is listed, with every state it exports (the ratchet)', () => {
		const problems: string[] = [];
		const files = Object.keys(modules).map(fileOf).sort();
		for (const f of files) {
			if (!STORES[f]) problems.push(`a store not listed in STORES: ${f}`);
		}
		for (const [f, names] of Object.entries(STORES)) {
			const m = modules[`./${f}`];
			if (!m) {
				problems.push(`listed in STORES, and no longer a store: ${f}`);
				continue;
			}
			for (const n of names) {
				if (typeof m[n] !== 'object' || m[n] === null) problems.push(`${f}: ${n} is not an exported state object`);
			}
			for (const [k, v] of Object.entries(m)) {
				if (typeof v === 'object' && v !== null && !names.includes(k)) {
					problems.push(`${f}: an exported object not listed in STORES: ${k}`);
				}
			}
		}
		expect(problems, problems.join('\n')).toEqual([]);
	});

	it('after a login, every store is in its initial state', () => {
		const states: [string, unknown][] = [];
		for (const [f, names] of Object.entries(STORES)) {
			const m = modules[`./${f}`];
			if (!m) continue;
			for (const n of names) if (!KEPT[n]) states.push([n, m[n]]);
		}
		const initial = new Map(states.map(([n, s]) => [n, comparable(n, s)]));
		for (const [n, s] of states) fill(n, s);
		// Precondition: the fill changed every state it reached.
		for (const [n, s] of states) {
			expect(comparable(n, s), `the test could not fill ${n}`).not.toEqual(initial.get(n));
		}

		let error = '';
		try {
			resetSessionState();
		} catch (e) {
			error = String(e);
		}
		const left = states.filter(([n, s]) => !isDeepStrictEqual(comparable(n, s), initial.get(n)));
		const problems = left.map(([n]) => `${n} still holds the previous session's state`);
		if (error) problems.unshift(`the reset failed: ${error}`);
		expect(problems, problems.join('\n')).toEqual([]);
	});

	it("the login runs the reset before it shows the unlocked app", () => {
		const src = loginScreen as unknown as string;
		const reset = src.indexOf('resetSessionState(');
		const ready = src.indexOf("app.authState = 'ready'");
		expect(reset, 'LoginScreen.svelte calls resetSessionState()').toBeGreaterThan(-1);
		expect(reset, 'before it sets authState to ready').toBeLessThan(ready);
	});
});
