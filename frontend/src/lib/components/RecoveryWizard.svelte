<script lang="ts">
	/** Surface 2 — Guardian Access Recovery wizard (pre-login, resumable).
	 *
	 *  The user forgot their password. Their guardians each release a share
	 *  of the Recovery Key; once 3 of 5 shares are in, the user sets a NEW
	 *  password and the account is re-wrapped and unlocked. There is no old
	 *  password to type and no data to reassemble — guardians restore
	 *  access, never the data (that's already on this device, synced).
	 *
	 *  Flow: start → wait (shares X/3 + per-guardian released + 72h
	 *  explainer) → at `ready`, set a NEW password → finalize → installed →
	 *  session unlocked. State is on disk, so the wizard resumes across the
	 *  multi-day wait. Backend: F1 access_recovery_* (feat/backup-m1-security).
	 */
	import { onMount } from 'svelte';
	import {
		recovery,
		start,
		finalize,
		cancel,
		checkNow,
		closeRecovery
	} from '$lib/stores/recovery.svelte';

	let newPassword = $state('');
	let confirmPassword = $state('');
	let showPw = $state(false);

	const phase = $derived(recovery.status?.phase ?? 'identify');

	// A 1s tick so the "next check in Ns" countdown actually counts down.
	// Without it the wait shows nothing for 45s and reads as stuck.
	let now = $state(Date.now());
	onMount(() => {
		const t = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(t);
	});
	const secondsToNextCheck = $derived(
		recovery.nextPollAt ? Math.max(0, Math.ceil((recovery.nextPollAt - now) / 1000)) : null
	);

	const passwordsMatch = $derived(
		newPassword.length > 0 && newPassword === confirmPassword
	);

	async function handleStart() {
		if (recovery.starting) return;
		await start();
	}

	async function handleFinalize() {
		if (!passwordsMatch || recovery.finalizing) return;
		const ok = await finalize(newPassword);
		newPassword = '';
		confirmPassword = '';
		if (ok) showPw = false;
	}

	async function handleAbandon() {
		if (
			!confirm(
				'Abandon this recovery? Any guardian who already released a share will need to release again if you restart.'
			)
		) {
			return;
		}
		await cancel();
	}
</script>

<div class="recovery-overlay">
	<div class="recovery-card">
		<button class="close-btn" onclick={closeRecovery} aria-label="Close recovery">
			&#x2715;
		</button>
		<h1 class="title">Recover your account</h1>

		{#if phase === 'identify'}
			<p class="subtitle">
				Forgot your password? Your guardians can let you back in. Each of
				them approves in person — recognising you by the proof you agreed
				when you set this up — and releases their share of your recovery
				key. Once 3 of your 5 guardians have released, you'll set a new
				password here.
			</p>
			<div class="form">
				<button class="primary-btn" disabled={recovery.starting} onclick={handleStart}>
					{recovery.starting ? 'Starting…' : 'Start recovery'}
				</button>
			</div>
			<p class="hint">
				This takes time by design: each guardian has a 72-hour window before
				their share is released — the delay that lets the real you stop an
				impostor. Expect days, not minutes. You can close this and come back;
				it keeps going.
			</p>
			{#if recovery.error}
				<p class="error">{recovery.error}</p>
			{/if}

		{:else if phase === 'awaiting_shares'}
			<p class="phase-label">Waiting for your guardians</p>
			{#if recovery.status}
				{@const s = recovery.status}
				<div class="progress-cell">
					<span class="progress-num">{s.shares_collected}/{s.threshold}</span>
					<span class="progress-what">recovery-key shares released</span>
				</div>
				<ul class="guardian-list">
					{#each s.guardians as g (g.guardian_id)}
						<li class="guardian-row">
							<span class="guardian-name">{g.guardian_id}</span>
							<span class="guardian-state {g.released ? 'released' : 'pending'}">
								{g.released ? 'released' : 'waiting'}
							</span>
						</li>
					{/each}
				</ul>
				<p class="hint">
					Reach your guardians in person or by phone and prove who you are
					the way you both agreed — a shared memory, an object, a private
					question. Each approval starts a 72-hour clock before the share
					releases; that window is your protection if this isn't really you.
				</p>
			{/if}

			<!-- The wait is long and quiet; say what's happening rather than
			     leave a still screen that reads as stuck. -->
			<div class="poll-row">
				<span class="poll-status" aria-live="polite">
					{#if recovery.polling}
						Checking with your guardians…
					{:else if secondsToNextCheck !== null}
						Next check in {secondsToNextCheck}s
					{:else}
						Waiting…
					{/if}
				</span>
				<button class="link-btn" disabled={recovery.polling} onclick={checkNow}>
					Check now
				</button>
			</div>

			<div class="footer-actions">
				<button class="link-btn" onclick={closeRecovery}>Close — keeps running</button>
				<button class="link-btn danger" onclick={handleAbandon}>Abandon</button>
			</div>

		{:else if phase === 'ready'}
			<p class="phase-label">Set a new password</p>
			<p class="subtitle">
				Enough of your guardians have released their shares. Choose a new
				password — this replaces the one you forgot. Your synced data
				unlocks with it right away.
			</p>
			<div class="form">
				<div class="pw-field">
					<input
						type={showPw ? 'text' : 'password'}
						class="text-input"
						placeholder="New password"
						bind:value={newPassword}
						disabled={recovery.finalizing}
						autocapitalize="off"
						autocorrect="off"
						autocomplete="new-password"
						spellcheck="false"
					/>
					<button
						type="button"
						class="reveal-btn"
						onclick={() => (showPw = !showPw)}
						tabindex="-1"
						aria-label={showPw ? 'Hide password' : 'Show password'}
					>
						{showPw ? 'Hide' : 'Show'}
					</button>
				</div>
				<input
					type={showPw ? 'text' : 'password'}
					class="text-input"
					placeholder="Confirm new password"
					bind:value={confirmPassword}
					disabled={recovery.finalizing}
					autocapitalize="off"
					autocorrect="off"
					autocomplete="new-password"
					spellcheck="false"
					onkeydown={(e) => e.key === 'Enter' && handleFinalize()}
				/>
				<button
					class="primary-btn"
					disabled={!passwordsMatch || recovery.finalizing}
					onclick={handleFinalize}
				>
					{recovery.finalizing ? 'Restoring…' : 'Restore my account'}
				</button>
			</div>
			{#if newPassword.length > 0 && !passwordsMatch}
				<p class="hint">The two passwords don't match yet.</p>
			{/if}
			{#if recovery.error}
				<p class="error">{recovery.error}</p>
			{/if}

		{:else if phase === 'installed'}
			<p class="phase-label">You're back in</p>
			<p class="subtitle">
				Your account is recovered and unlocked with your new password. From
				here on, log in with it.
			</p>
			<button class="primary-btn" onclick={closeRecovery}>Continue</button>

		{:else if phase === 'failed'}
			<p class="phase-label">Recovery failed</p>
			{#if recovery.status?.error}
				<p class="error">{recovery.status.error}</p>
			{/if}
			<button class="secondary-btn" onclick={handleAbandon}>Discard and start over</button>
		{/if}
	</div>
</div>

<style>
	.recovery-overlay {
		position: fixed;
		inset: 0;
		z-index: 1100; /* above the login overlay (1000) */
		background: var(--bg-primary, #1a1a20);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.recovery-card {
		position: relative;
		width: 460px;
		max-height: 86vh;
		overflow-y: auto;
		padding: 44px 40px;
		background: var(--bg-panel, #242430);
		border: 1px solid var(--border, #333);
		border-radius: 16px;
	}

	.close-btn {
		position: absolute;
		top: 14px;
		right: 14px;
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 1rem;
		cursor: pointer;
	}
	.close-btn:hover {
		color: var(--text-primary, #e0e0e0);
	}

	.title {
		font-size: 1.4rem;
		font-weight: 700;
		color: var(--accent, #4ea7e9);
		margin: 0 0 10px 0;
	}

	.subtitle {
		color: var(--text-secondary, #888);
		font-size: 0.9rem;
		line-height: 1.55;
		margin: 0 0 20px 0;
	}

	.phase-label {
		color: var(--text-primary, #e0e0e0);
		font-size: 1.02rem;
		font-weight: 600;
		margin: 0 0 16px 0;
	}

	.form {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-bottom: 14px;
	}

	.text-input {
		width: 100%;
		padding: 12px 16px;
		background: var(--bg-input, #1a1a20);
		border: 1px solid var(--border, #333);
		border-radius: 8px;
		color: var(--text-primary, #e0e0e0);
		font-size: 0.95rem;
		outline: none;
		box-sizing: border-box;
	}
	.text-input:focus {
		border-color: var(--accent, #4ea7e9);
	}

	.pw-field {
		position: relative;
	}
	.pw-field .text-input {
		padding-right: 64px;
	}
	.reveal-btn {
		position: absolute;
		right: 6px;
		top: 50%;
		transform: translateY(-50%);
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 0.85rem;
		cursor: pointer;
		padding: 6px 10px;
	}
	.reveal-btn:hover {
		color: var(--accent, #4ea7e9);
	}

	.primary-btn {
		padding: 12px;
		background: var(--accent, #4ea7e9);
		color: #000;
		border: none;
		border-radius: 8px;
		font-size: 0.95rem;
		font-weight: 600;
		cursor: pointer;
	}
	.primary-btn:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}

	.secondary-btn {
		padding: 10px 16px;
		background: var(--bg-input, #1a1a20);
		color: var(--text-primary, #e0e0e0);
		border: 1px solid var(--border, #333);
		border-radius: 8px;
		font-size: 0.9rem;
		cursor: pointer;
	}

	.hint {
		color: var(--text-muted, #666);
		font-size: 0.8rem;
		line-height: 1.5;
		margin: 12px 0 0 0;
	}

	.error {
		color: var(--error, #ef4444);
		font-size: 0.85rem;
		margin: 12px 0 0 0;
	}

	.progress-cell {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		background: var(--bg-input, #1a1a20);
		border: 1px solid var(--border, #333);
		border-radius: 10px;
		padding: 14px;
		margin-bottom: 14px;
	}
	.progress-num {
		color: var(--accent, #4ea7e9);
		font-size: 1.6rem;
		font-weight: 700;
	}
	.progress-what {
		color: var(--text-muted, #666);
		font-size: 0.78rem;
	}

	.guardian-list {
		list-style: none;
		margin: 0 0 4px 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.guardian-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 12px;
		background: var(--bg-input, #1a1a20);
		border: 1px solid var(--border, #333);
		border-radius: 8px;
		padding: 8px 12px;
	}
	.guardian-name {
		color: var(--text-primary, #e0e0e0);
		font-size: 0.8rem;
		font-family: monospace;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.guardian-state {
		font-size: 0.75rem;
		flex: none;
	}
	.guardian-state.pending {
		color: var(--text-muted, #666);
	}
	.guardian-state.released {
		color: var(--success, #10b981);
	}

	.poll-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin-top: 14px;
		padding-top: 12px;
		border-top: 1px solid var(--border, #333);
	}
	.poll-status {
		color: var(--text-muted, #666);
		font-size: 0.78rem;
	}

	.footer-actions {
		display: flex;
		justify-content: space-between;
		margin-top: 18px;
	}
	.link-btn {
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 0.8rem;
		cursor: pointer;
		padding: 4px 0;
	}
	.link-btn:hover {
		color: var(--text-primary, #e0e0e0);
	}
	.link-btn.danger:hover {
		color: var(--error, #ef4444);
	}
</style>
