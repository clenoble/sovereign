/** Invariants of the pre-login access-recovery wizard against the frozen
 *  recovery session API (SEAM-A: start(new_passphrase) / resume / poll /
 *  finalize, `"wrong-passphrase"` sentinel). These assert what must hold for
 *  the wizard to conform to the backend contract — not a desired outcome of
 *  any particular recovery. */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mockTauriCommand } from '$lib/test/tauri';
import type { AccessRecoveryStatusDto } from '$lib/api/commands';
import {
	recovery,
	stepOf,
	openRecovery,
	closeRecovery,
	start,
	resume,
	finalize,
	cancel,
	checkNow,
	WRONG_PASSPHRASE_MESSAGE
} from './recovery.svelte';

type Calls = Record<string, unknown[]>;
let calls: Calls;

function status(phase: AccessRecoveryStatusDto['phase'], shares = 0): AccessRecoveryStatusDto {
	return {
		recovery_id: 'acc-1',
		phase,
		shares_collected: shares,
		threshold: 3,
		guardians: [
			{ guardian_id: 'g1', released: shares > 0 },
			{ guardian_id: 'g2', released: false }
		],
		error: null
	};
}

/** Register a command handler that records every call's arguments. */
function mock(cmd: string, handler: (args: unknown) => unknown) {
	mockTauriCommand(cmd, (args: unknown) => {
		(calls[cmd] ??= []).push(args);
		return handler(args);
	});
}

const count = (cmd: string) => calls[cmd]?.length ?? 0;
const flush = () => vi.advanceTimersByTimeAsync(0);

beforeEach(() => {
	vi.useFakeTimers();
	calls = {};
	Object.assign(recovery, {
		visible: false,
		available: false,
		status: null,
		sessionOpen: false,
		starting: false,
		resuming: false,
		finalizing: false,
		polling: false,
		nextPollAt: null,
		error: null
	});
	mock('validate_password_policy', () => ({ valid: true, errors: [] }));
	mock('access_recovery_poll', () => status('awaiting_shares', 1));
	mock('cancel_access_recovery', () => undefined);
});

afterEach(() => {
	closeRecovery(); // clears any armed poll timer
	vi.useRealTimers();
});

describe('stepOf', () => {
	it('maps status + session to the wizard step', () => {
		expect(stepOf(null, false)).toBe('choose-passphrase');
		expect(stepOf(null, true)).toBe('choose-passphrase');
		expect(stepOf(status('awaiting_shares'), false)).toBe('resume');
		expect(stepOf(status('ready', 3), false)).toBe('resume');
		expect(stepOf(status('awaiting_shares'), true)).toBe('awaiting');
		expect(stepOf(status('ready', 3), true)).toBe('ready');
	});

	it('needs no passphrase for terminal phases', () => {
		expect(stepOf(status('failed'), false)).toBe('failed');
		expect(stepOf(status('installed', 3), false)).toBe('installed');
	});
});

describe('start', () => {
	it('never calls start without a new passphrase', async () => {
		mock('start_access_recovery', () => status('awaiting_shares'));

		expect(await start('', '')).toBe(false);
		expect(count('start_access_recovery')).toBe(0);
		expect(recovery.error).toBeTruthy();
	});

	it('never calls start when the confirmation differs', async () => {
		mock('start_access_recovery', () => status('awaiting_shares'));

		expect(await start('Correct-Horse-9!', 'Correct-Horse-9?')).toBe(false);
		expect(count('start_access_recovery')).toBe(0);
		expect(recovery.error).toMatch(/match/);
	});

	it('never calls start with a password the policy rejects', async () => {
		mock('validate_password_policy', () => ({ valid: false, errors: ['Too short.'] }));
		mock('start_access_recovery', () => status('awaiting_shares'));

		expect(await start('short', 'short')).toBe(false);
		expect(count('start_access_recovery')).toBe(0);
		expect(recovery.error).toMatch(/Too short/);
	});

	it('passes the new passphrase to start, opens the session, then polls', async () => {
		mock('start_access_recovery', () => status('awaiting_shares'));

		expect(await start('Correct-Horse-9!', 'Correct-Horse-9!')).toBe(true);
		expect(calls['start_access_recovery']).toEqual([{ newPassphrase: 'Correct-Horse-9!' }]);
		expect(recovery.sessionOpen).toBe(true);

		await flush();
		expect(count('access_recovery_poll')).toBe(1);
		expect(recovery.status?.shares_collected).toBe(1);
		expect(recovery.nextPollAt).not.toBeNull();
	});
});

describe('resume', () => {
	it('offers resume for an in-progress recovery and does not poll before it', async () => {
		mock('access_recovery_status', () => status('awaiting_shares', 1));

		await openRecovery();
		await flush();

		expect(recovery.visible).toBe(true);
		expect(stepOf(recovery.status, recovery.sessionOpen)).toBe('resume');
		expect(count('access_recovery_poll')).toBe(0);
		expect(recovery.nextPollAt).toBeNull();
	});

	it('keeps polling on reopen within the same run (session already open)', async () => {
		mock('start_access_recovery', () => status('awaiting_shares'));
		mock('access_recovery_status', () => status('awaiting_shares', 1));
		await start('Correct-Horse-9!', 'Correct-Horse-9!');
		await flush();
		closeRecovery();
		const before = count('access_recovery_poll');

		await openRecovery();
		await flush();

		expect(stepOf(recovery.status, recovery.sessionOpen)).toBe('awaiting');
		expect(count('access_recovery_poll')).toBe(before + 1);
	});

	it('re-prompts on the wrong-passphrase sentinel without opening the session', async () => {
		mock('access_recovery_status', () => status('awaiting_shares', 1));
		mock('resume_access_recovery', () => {
			// Tauri rejects with the command's Err string as-is.
			throw 'wrong-passphrase';
		});
		await openRecovery();

		expect(await resume('not-it')).toBe(false);
		expect(calls['resume_access_recovery']).toEqual([{ passphrase: 'not-it' }]);
		expect(recovery.error).toBe(WRONG_PASSPHRASE_MESSAGE);
		expect(recovery.sessionOpen).toBe(false);
		await flush();
		expect(count('access_recovery_poll')).toBe(0);
	});

	it('opens the session on the right passphrase and resumes polling', async () => {
		mock('access_recovery_status', () => status('awaiting_shares', 1));
		mock('resume_access_recovery', () => status('awaiting_shares', 1));
		await openRecovery();

		expect(await resume('Correct-Horse-9!')).toBe(true);
		expect(recovery.sessionOpen).toBe(true);
		await flush();
		expect(count('access_recovery_poll')).toBe(1);
	});

	it('never calls resume with an empty passphrase', async () => {
		mock('resume_access_recovery', () => status('awaiting_shares'));
		expect(await resume('')).toBe(false);
		expect(count('resume_access_recovery')).toBe(0);
	});

	it('falls back to the resume step when the backend reports no session', async () => {
		mock('start_access_recovery', () => status('awaiting_shares'));
		mock('access_recovery_poll', () => {
			throw 'recovery session not open — resume with your new passphrase first';
		});
		await start('Correct-Horse-9!', 'Correct-Horse-9!');
		await flush();

		expect(recovery.sessionOpen).toBe(false);
		expect(stepOf(recovery.status, recovery.sessionOpen)).toBe('resume');
		expect(recovery.nextPollAt).toBeNull();
		await checkNow();
		expect(count('access_recovery_poll')).toBe(1); // no further round
	});
});

describe('finalize', () => {
	it('sends no passphrase argument (the backend uses the session)', async () => {
		mock('start_access_recovery', () => status('ready', 3));
		mock('access_recovery_finalize', () => status('installed', 3));
		await start('Correct-Horse-9!', 'Correct-Horse-9!');

		expect(await finalize()).toBe(true);
		expect(calls['access_recovery_finalize']).toEqual([undefined]);
		expect(recovery.sessionOpen).toBe(false);
	});

	it('is not called before the session is open', async () => {
		mock('access_recovery_finalize', () => status('installed', 3));
		recovery.status = status('ready', 3);

		expect(await finalize()).toBe(false);
		expect(count('access_recovery_finalize')).toBe(0);
	});

	it('does not poll once enough shares are in', async () => {
		mock('start_access_recovery', () => status('ready', 3));
		await start('Correct-Horse-9!', 'Correct-Horse-9!');
		await flush();
		expect(count('access_recovery_poll')).toBe(0);
	});

	it('shows a failure recorded on disk', async () => {
		mock('start_access_recovery', () => status('ready', 3));
		mock('access_recovery_finalize', () => {
			throw 'reconstruction failed';
		});
		mock('access_recovery_status', () => ({ ...status('failed', 3), error: 'reconstruction failed' }));
		await start('Correct-Horse-9!', 'Correct-Horse-9!');

		expect(await finalize()).toBe(false);
		expect(stepOf(recovery.status, recovery.sessionOpen)).toBe('failed');
	});
});

describe('cancel', () => {
	it('drops the session and the status', async () => {
		mock('start_access_recovery', () => status('awaiting_shares'));
		await start('Correct-Horse-9!', 'Correct-Horse-9!');
		await flush();

		await cancel();
		expect(count('cancel_access_recovery')).toBe(1);
		expect(recovery.sessionOpen).toBe(false);
		expect(recovery.status).toBeNull();
		expect(recovery.nextPollAt).toBeNull();
	});
});
