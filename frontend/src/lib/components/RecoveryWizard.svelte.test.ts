/** The access-recovery wizard asks for the new password before anything
 *  else, offers resume for an in-progress recovery, and asks for no password
 *  at finalize (SEAM-A). Fields are reachable by their visible label. */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import type { AccessRecoveryStatusDto } from '$lib/api/commands';
import RecoveryWizard from './RecoveryWizard.svelte';
import { recovery, closeRecovery } from '$lib/stores/recovery.svelte';

function status(phase: AccessRecoveryStatusDto['phase'], shares = 0): AccessRecoveryStatusDto {
	return {
		recovery_id: 'acc-1',
		phase,
		shares_collected: shares,
		threshold: 3,
		guardians: [{ guardian_id: 'g1', released: shares > 0 }],
		error: null
	};
}

let startArgs: unknown[];

beforeEach(() => {
	startArgs = [];
	Object.assign(recovery, {
		visible: true,
		available: true,
		status: null,
		sessionOpen: false,
		starting: false,
		resuming: false,
		finalizing: false,
		polling: false,
		nextPollAt: null,
		error: null
	});
	mockTauriCommand('validate_password_policy', () => ({ valid: true, errors: [] }));
	mockTauriCommand('start_access_recovery', (args) => {
		startArgs.push(args);
		// 'ready' so no poll round is scheduled during the test.
		return status('ready', 3);
	});
});

afterEach(() => closeRecovery());

describe('RecoveryWizard', () => {
	it('asks for the new password first and passes it to start', async () => {
		const { getByLabelText, getByRole } = render(RecoveryWizard);
		const pw = getByLabelText('New password') as HTMLInputElement;
		const confirm = getByLabelText('Confirm new password') as HTMLInputElement;
		const submit = getByRole('button', { name: 'Start recovery' }) as HTMLButtonElement;

		expect(submit.disabled).toBe(true);
		await fireEvent.input(pw, { target: { value: 'Correct-Horse-9!' } });
		await fireEvent.input(confirm, { target: { value: 'Correct-Horse-9?' } });
		expect(submit.disabled).toBe(true);
		expect(getByRole('dialog').textContent).toContain("don't match");

		await fireEvent.input(confirm, { target: { value: 'Correct-Horse-9!' } });
		expect(submit.disabled).toBe(false);
		await fireEvent.click(submit);
		await new Promise((r) => setTimeout(r, 0));
		await tick();

		expect(startArgs).toEqual([{ newPassphrase: 'Correct-Horse-9!' }]);
	});

	it('offers resume for an in-progress recovery with one labelled field', async () => {
		recovery.status = status('awaiting_shares', 1);
		const { getByLabelText, getByRole, queryByLabelText } = render(RecoveryWizard);

		expect(getByLabelText('The new password you chose')).toBeTruthy();
		expect(getByRole('button', { name: 'Resume' })).toBeTruthy();
		expect(queryByLabelText('Confirm new password')).toBeNull();
	});

	it('asks for no password once the session is open and shares are in', async () => {
		recovery.status = status('ready', 3);
		recovery.sessionOpen = true;
		const { getByRole, container } = render(RecoveryWizard);

		expect(getByRole('button', { name: 'Restore my account' })).toBeTruthy();
		expect(container.querySelectorAll('input[type="password"]').length).toBe(0);
	});
});
