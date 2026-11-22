/** C-SUP on the app built with P2P (spec ch. 05, *The node's supervision*):
 *  SU2, the words, the same on both hosts (the Fable maille, by delegation of
 *  Céline, 2026-11-15), and SU5, an end the session asked for says nothing.
 *  Written before the code, by another author; the implementer does not edit
 *  this file. The backend's side (the node under supervision, the event, the
 *  restart command) is held by `crates/sovereign-app/tests/csup_call_sites.rs`.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { resetSessionState } from '$lib/session';
import {
	dismissSyncStoppedNotice,
	onRestartFailed,
	onGestureNeedingTheNode,
	onSessionEnded,
	onSyncStateAtLogin,
	onSyncRestarted,
	onSyncStopped,
	syncStoppedNotice,
	syncStoppedStatus
} from './syncStopped.svelte';

const STATUS = 'Sync stopped';
const TITLE = 'Sync stopped';
const TEXT = 'Sync stopped unexpectedly. Nothing syncs until you restart it.';
const GESTURE = 'Restart sync';

/** Each test starts from a session in which sync runs. */
function freshSession() {
	onSyncRestarted();
	onSessionEnded();
}

describe('C-SUP SU2: the app says that sync stopped', () => {
	beforeEach(freshSession);

	it('says nothing while sync runs', () => {
		expect(syncStoppedStatus()).toBeNull();
		expect(syncStoppedNotice()).toBeNull();
	});

	it('says « Sync stopped » in its status line, and once the notice, with the gesture', () => {
		onSyncStopped();
		expect(syncStoppedStatus()).toBe(STATUS);
		expect(syncStoppedNotice()).toEqual({ title: TITLE, text: TEXT, gesture: GESTURE });
	});

	it('shows the notice once per stop: closed, it stays closed while the status line still says it', () => {
		onSyncStopped();
		dismissSyncStoppedNotice();
		expect(syncStoppedNotice()).toBeNull();
		expect(syncStoppedStatus()).toBe(STATUS);
		// A restart, then a new stop: a new notice.
		onSyncRestarted();
		expect(syncStoppedStatus()).toBeNull();
		onSyncStopped();
		expect(syncStoppedNotice()?.text).toBe(TEXT);
	});

	it('says a restart that fails, with its reason, and keeps the gesture', () => {
		onSyncStopped();
		onRestartFailed('the account key is not available');
		const n = syncStoppedNotice();
		expect(n?.text).toBe('Sync could not be restarted: the account key is not available');
		expect(n?.gesture).toBe(GESTURE);
		expect(syncStoppedStatus()).toBe(STATUS);
	});
});

describe('C-SUP SU5: an end the session asked for is not a stop', () => {
	beforeEach(freshSession);

	it('says nothing after a logout, a switch, a lock or the exit', () => {
		onSessionEnded();
		expect(syncStoppedStatus()).toBeNull();
		expect(syncStoppedNotice()).toBeNull();
	});

	// SU4 (spec ch. 05, v2.0.105; the Fable maille, by delegation of Céline, 2026-11-15): a stop ends with its session.
	it('ends the stopped state with its session: from a stop, a session end leaves no status, no notice, no gesture', () => {
		onSyncStopped();
		expect(syncStoppedStatus(), 'precondition: the status line says it').toBe(STATUS);
		expect(syncStoppedNotice()?.gesture, 'precondition: the notice shows, with its gesture').toBe(GESTURE);
		onSessionEnded();
		expect(syncStoppedStatus()).toBeNull();
		expect(syncStoppedNotice()).toBeNull();
	});
});

describe('C-SUP SU2, SU6: the event reaches the store', () => {
	it('the app listens for `sync-stopped` and hands it to onSyncStopped', () => {
		const here = dirname(fileURLToPath(import.meta.url));
		const events = readFileSync(join(here, '..', 'api', 'events.ts'), 'utf8');
		const at = events.indexOf("'sync-stopped'");
		expect(at, 'events.ts listens for the `sync-stopped` event').toBeGreaterThan(-1);
		expect(events.slice(at, at + 300)).toContain('onSyncStopped');
	});
});

describe('DEL-0019: a gesture that needs the node while sync is stopped', () => {
	beforeEach(freshSession);

	it("shows the stop's notice again, with its gesture, after it was closed", () => {
		onSyncStopped();
		dismissSyncStoppedNotice();
		expect(onGestureNeedingTheNode()).toBe(true);
		expect(syncStoppedNotice()).toEqual({ title: TITLE, text: TEXT, gesture: GESTURE });
		expect(syncStoppedStatus()).toBe(STATUS);
	});

	it('changes nothing when sync runs, or was never started', () => {
		expect(onGestureNeedingTheNode()).toBe(false);
		expect(syncStoppedNotice()).toBeNull();
		expect(syncStoppedStatus()).toBeNull();
	});
});

describe('SU1 (Céline, 2026-11-16): the stop reaches the screen after the login has answered', () => {
	beforeEach(freshSession);

	it("a stop recorded during the login is still said once the login's reset has run", () => {
		onSessionEnded(); // the previous session ended; the login begins
		onSyncStopped(); // the new session's start fails during the login
		resetSessionState(); // the login's reset (C-AREA AR4), run by LoginScreen.svelte
		onSyncStateAtLogin(true); // the login asks the backend: this session's sync is stopped
		expect(syncStoppedStatus()).toBe(STATUS);
		expect(syncStoppedNotice()).toMatchObject({ title: TITLE, text: TEXT, gesture: GESTURE });
	});

	it('a session whose sync was never stopped opens saying nothing', () => {
		onSessionEnded();
		resetSessionState();
		onSyncStateAtLogin(false);
		expect(syncStoppedStatus()).toBeNull();
		expect(syncStoppedNotice()).toBeNull();
	});
});

describe('SU1 (Céline, 2026-11-16): where the login asks', () => {
	it("the login asks after its reset, through the backend's `sync_is_stopped`", () => {
		const here = dirname(fileURLToPath(import.meta.url));
		const login = readFileSync(join(here, '..', 'components', 'LoginScreen.svelte'), 'utf8');
		const reset = login.indexOf('resetSessionState(');
		const ask = login.indexOf('onSyncStateAtLogin(');
		expect(reset, 'LoginScreen.svelte runs the reset').toBeGreaterThan(-1);
		expect(ask, 'LoginScreen.svelte hands the answer to onSyncStateAtLogin').toBeGreaterThan(-1);
		expect(ask, 'after the reset, so the reset cannot clear it').toBeGreaterThan(reset);
		const src = join(here, '..', '..');
		const all: string[] = [];
		const walk = (dir: string) => {
			for (const e of readdirSync(dir, { withFileTypes: true })) {
				const p = join(dir, e.name);
				if (e.isDirectory()) walk(p);
				else if (/\.(ts|svelte)$/.test(e.name) && !e.name.includes('.test.')) all.push(p);
			}
		};
		walk(src);
		expect(
			all.some((f) => readFileSync(f, 'utf8').includes("'sync_is_stopped'")),
			'the interface asks the backend through `sync_is_stopped`'
		).toBe(true);
	});
});

describe('SU2 (the Fable maille, by delegation of Céline, 2026-11-16): the notice closes with « Dismiss »', () => {
	it('every screen that shows the stop’s notice closes it with a button named « Dismiss », beside « Restart sync »', () => {
		const here = dirname(fileURLToPath(import.meta.url));
		const root = join(here, '..', '..');
		const found: string[] = [];
		const walk = (dir: string) => {
			for (const e of readdirSync(dir, { withFileTypes: true })) {
				const p = join(dir, e.name);
				if (e.isDirectory()) walk(p);
				else if (e.name.endsWith('.svelte')) found.push(p);
			}
		};
		walk(root);
		const showing = found.filter((f) => readFileSync(f, 'utf8').includes('syncStoppedNotice('));
		expect(showing.length, 'no screen shows the stop’s notice').toBeGreaterThan(0);
		for (const f of showing) {
			const src = readFileSync(f, 'utf8');
			expect(src, `${f}: the notice's close button is named « Dismiss »`).toContain('Dismiss');
			expect(src, `${f}: « Dismiss » closes the notice`).toContain('dismissSyncStoppedNotice');
			expect(src, `${f}: « Restart sync » beside it`).toContain('restartSync');
		}
	});
});
