<script lang="ts">
	import { app } from '$lib/stores/app.svelte';
	import { validatePassword, checkAuthState } from '$lib/api/commands';
	import type { KeystrokeSampleDto } from '$lib/api/commands';
	import { openRecovery, checkAvailable, recovery } from '$lib/stores/recovery.svelte';
	import { setSetupNoticeAfterLogin } from '$lib/stores/setupNotice.svelte';
	import { loadPiiSignal, offerPiiLoginNotice } from '$lib/stores/piiReview.svelte';
	import { resetSessionState } from '$lib/session';
	import { onSyncStateAtLogin } from '$lib/stores/syncStopped.svelte';
	import { syncIsStoppedCmd } from '$lib/api/commands';

	// The backend's one sentence for a wrong password (spec ch. 07, LE2, LE4).
	const WRONG_PASSWORD = 'Invalid password';
	// Shown when the login failed with no text: it is never read as a wrong password.
	const NO_REASON = 'The login failed without saying why. This is not a wrong password.';

	let password = $state('');
	let error = $state('');
	let attempts = $state(0);
	let maxAttempts = $state(10);
	let lockoutSeconds = $state(300);
	let lockedUntil = $state<number | null>(null);
	let lockCountdown = $state('');
	let submitting = $state(false);
	let showPw = $state(false);
	// P5g: what the backend reported when this screen opened. Captured before the login,
	// since the backend clears the marker at the first login of either persona. The login
	// waits for this answer, so an early submit still gets the notice; a failed check
	// counts as false and does not block the login.
	let setupIncompleteAtOpen: Promise<boolean> = Promise.resolve(false);

	// Keystroke timing capture
	let keyTimings: Map<string, number> = new Map();
	let keystrokes: KeystrokeSampleDto[] = [];

	// Load config for lockout settings
	$effect(() => {
		setupIncompleteAtOpen = checkAuthState()
			.then((result) => {
				// Config values come from the backend; defaults are fine
				return result.setup_incomplete === true; // P5g
			})
			.catch(() => false);
	});

	// Show the "forgot password?" recover option only if this device holds a
	// recovery card + bundle (i.e. guardians were enrolled before the loss).
	$effect(() => {
		checkAvailable();
	});

	// Lockout countdown timer
	$effect(() => {
		if (lockedUntil === null) {
			lockCountdown = '';
			return;
		}
		const interval = setInterval(() => {
			const remaining = Math.max(0, lockedUntil! - Date.now());
			if (remaining <= 0) {
				lockedUntil = null;
				lockCountdown = '';
				clearInterval(interval);
				return;
			}
			const mins = Math.floor(remaining / 60000);
			const secs = Math.floor((remaining % 60000) / 1000);
			lockCountdown = `${mins}:${secs.toString().padStart(2, '0')}`;
		}, 250);
		return () => clearInterval(interval);
	});

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			handleSubmit();
			return;
		}
		// Typing timings are optional (spec ch. 07): an event without a key — what a
		// system autofill dispatches — is ignored, never recorded as a sample.
		if (!hasKey(e)) return;
		keyTimings.set(e.key, Date.now());
	}

	function hasKey(e: KeyboardEvent): boolean {
		return typeof e.key === 'string' && e.key.length > 0;
	}

	function handleKeyup(e: KeyboardEvent) {
		if (!hasKey(e)) return;
		const pressTime = keyTimings.get(e.key);
		if (pressTime) {
			keystrokes.push({
				key: e.key,
				press_ms: pressTime,
				release_ms: Date.now()
			});
			keyTimings.delete(e.key);
		}
	}

	async function handleSubmit() {
		// Trim before sending: phone keyboards can slip an invisible leading/
		// trailing space into the field (undetectable even with reveal on),
		// which makes the correct password fail to authenticate. The guard
		// already trims, but we previously sent the untrimmed value.
		const pw = password.trim();
		if (!pw || submitting || lockedUntil) return;
		submitting = true;
		error = '';

		try {
			const setupIncomplete = await setupIncompleteAtOpen; // P5g: before the login clears it
			const persona = await validatePassword(pw, keystrokes);
			// C-AREA AR4: the interface starts from nothing at a switch (spec ch. 07):
			// nothing of a previous session's screens survives this login.
			resetSessionState();
			// C-SUP SU1: a stop of the session this login opened (its node did
			// not start) is asked for after the reset, so the reset cannot clear it.
			// An error of the question says nothing: the first gesture that
			// needs the node shows the stop's notice anyway.
			void syncIsStoppedCmd()
				.then((stopped) => onSyncStateAtLogin(stopped))
				.catch(() => {});
			setSetupNoticeAfterLogin(setupIncomplete); // P5g: shown in the unlocked app, not here
			// C-SWEEP SW6: once the session is open, the queue's count and the
			// notice at login, after the login's other notices (it yields to
			// them). A failure says nothing: the count is read again later.
			void offerPiiLoginNotice().catch(() => {});
			void loadPiiSignal().catch(() => {});
			app.authState = 'ready';
		} catch (e) {
			// Surface the REAL backend error (spec ch. 07, login errors, LE4).
			// validate_password says exactly "Invalid password" for a wrong
			// password and for nothing else; only that counts toward the
			// lockout. Any other text is shown as the backend gave it, even one
			// that holds those words inside a longer sentence. An error with no
			// text is not a wrong password either: it says so, and is not counted.
			const raw = typeof e === 'string' ? e : (e as { message?: unknown } | null | undefined)?.message;
			const msg = typeof raw === 'string' ? raw : '';
			password = '';
			keystrokes = [];

			if (msg === WRONG_PASSWORD) {
				attempts++;
				if (attempts >= maxAttempts) {
					lockedUntil = Date.now() + lockoutSeconds * 1000;
					error = '';
				} else {
					error = WRONG_PASSWORD;
				}
			} else {
				error = msg.trim() ? msg : NO_REASON;
			}
		} finally {
			submitting = false;
		}
	}
</script>

<div class="login-overlay">
	<div class="login-card">
		<h1 class="title">Sovereign GE</h1>
		<p class="subtitle">Enter your password to unlock</p>

		{#if lockedUntil}
			<div class="lockout">
				<p class="lockout-title">Account Locked</p>
				<p class="lockout-timer">Try again in {lockCountdown}</p>
			</div>
		{:else}
			<div class="form">
				<div class="password-field">
					<input
						type={showPw ? 'text' : 'password'}
						class="password-input"
						aria-label="Password"
						placeholder="Password"
						bind:value={password}
						onkeydown={handleKeydown}
						onkeyup={handleKeyup}
						disabled={submitting}
						autocapitalize="off"
						autocorrect="off"
						autocomplete="current-password"
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
				<button
					class="unlock-btn"
					onclick={handleSubmit}
					disabled={!password.trim() || submitting}
				>
					{submitting ? 'Unlocking...' : 'Unlock'}
				</button>
			</div>

			{#if error}
				<p class="error" role="alert">{error}</p>
			{/if}

			{#if attempts > 0 && !lockedUntil}
				<p class="attempts">{maxAttempts - attempts} attempts remaining</p>
			{/if}

			{#if recovery.available}
				<button class="recover-link" onclick={openRecovery}>
					Forgot your password? Recover with your guardians
				</button>
			{/if}
		{/if}
	</div>
</div>

<style>
	.login-overlay {
		position: fixed;
		inset: 0;
		z-index: 1000;
		background: var(--bg-primary, #1a1a20);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.login-card {
		width: 420px;
		padding: 48px 40px;
		background: var(--bg-panel, #242430);
		border: 1px solid var(--border, #333);
		border-radius: 16px;
		text-align: center;
	}

	.title {
		font-size: 1.6rem;
		font-weight: 700;
		color: var(--accent, #4ea7e9);
		margin: 0 0 8px 0;
	}

	.subtitle {
		color: var(--text-secondary, #888);
		font-size: 0.9rem;
		margin: 0 0 32px 0;
	}

	.form {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.password-input {
		width: 100%;
		padding: 12px 16px;
		background: var(--bg-input, #1a1a20);
		border: 1px solid var(--border, #333);
		border-radius: 8px;
		color: var(--text-primary, #e0e0e0);
		font-size: 1rem;
		outline: none;
		box-sizing: border-box;
	}
	.password-input:focus {
		border-color: var(--accent, #4ea7e9);
	}

	.password-field {
		position: relative;
	}
	.password-field .password-input {
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

	.unlock-btn {
		padding: 12px;
		background: var(--accent, #4ea7e9);
		color: #000;
		border: none;
		border-radius: 8px;
		font-size: 1rem;
		font-weight: 600;
		cursor: pointer;
	}
	.unlock-btn:hover:not(:disabled) {
		background: var(--accent-hover, #3a8fce);
	}
	.unlock-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.error {
		color: var(--error, #ef4444);
		font-size: 0.85rem;
		margin: 16px 0 0 0;
	}

	.attempts {
		color: var(--text-muted, #666);
		font-size: 0.8rem;
		margin: 8px 0 0 0;
	}

	.recover-link {
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 0.8rem;
		cursor: pointer;
		margin-top: 20px;
		padding: 4px 0;
		text-decoration: underline;
	}
	.recover-link:hover {
		color: var(--accent, #4ea7e9);
	}

	.lockout {
		padding: 24px 0;
	}
	.lockout-title {
		color: var(--error, #ef4444);
		font-size: 1.1rem;
		font-weight: 600;
		margin: 0 0 8px 0;
	}
	.lockout-timer {
		color: var(--text-secondary, #888);
		font-size: 1.4rem;
		font-family: monospace;
		margin: 0;
	}
</style>
