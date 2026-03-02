<script lang="ts">
	import { authState } from '$lib/stores/app';
	import { validatePassword, checkAuthState } from '$lib/api/commands';
	import type { KeystrokeSampleDto } from '$lib/api/commands';

	let password = $state('');
	let error = $state('');
	let attempts = $state(0);
	let maxAttempts = $state(10);
	let lockoutSeconds = $state(300);
	let lockedUntil = $state<number | null>(null);
	let lockCountdown = $state('');
	let submitting = $state(false);

	// Keystroke timing capture
	let keyTimings: Map<string, number> = new Map();
	let keystrokes: KeystrokeSampleDto[] = [];

	// Load config for lockout settings
	$effect(() => {
		checkAuthState().then((result) => {
			// Config values come from the backend; defaults are fine
		});
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
		keyTimings.set(e.key, Date.now());
	}

	function handleKeyup(e: KeyboardEvent) {
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
		if (!password.trim() || submitting || lockedUntil) return;
		submitting = true;
		error = '';

		try {
			const persona = await validatePassword(password, keystrokes);
			authState.set('ready');
		} catch (e) {
			attempts++;
			error = 'Invalid password';
			password = '';
			keystrokes = [];

			if (attempts >= maxAttempts) {
				lockedUntil = Date.now() + lockoutSeconds * 1000;
				error = '';
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
				<input
					type="password"
					class="password-input"
					placeholder="Password"
					bind:value={password}
					onkeydown={handleKeydown}
					onkeyup={handleKeyup}
					disabled={submitting}
				/>
				<button
					class="unlock-btn"
					onclick={handleSubmit}
					disabled={!password.trim() || submitting}
				>
					{submitting ? 'Unlocking...' : 'Unlock'}
				</button>
			</div>

			{#if error}
				<p class="error">{error}</p>
			{/if}

			{#if attempts > 0 && !lockedUntil}
				<p class="attempts">{maxAttempts - attempts} attempts remaining</p>
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
