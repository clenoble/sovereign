<script lang="ts">
	import { onMount } from 'svelte';
	import { app } from '$lib/stores/app.svelte';
	import {
		checkAuthState,
		validatePasswordPolicy,
		completeOnboarding,
		toggleTheme
	} from '$lib/api/commands';
	import type { KeystrokeSampleDto, OnboardingData } from '$lib/api/commands';
	import BubblePreview from './BubblePreview.svelte';
	import { applyTheme, theme } from '$lib/stores/theme.svelte';

	// ---------------------------------------------------------------------------
	// Designation generator
	// ---------------------------------------------------------------------------
	const LATIN = 'ABCDEFGHJKMNPQRSTUVWXYZ23456789';
	const NON_LATIN = [
		'\u03A9', '\u0394', '\u03A3', '\u039B', '\u03A0', '\u03B8', '\u03C6',
		'\u0416', '\u042F', '\u0429',
		'\u0924', '\u0915', '\u0926',
		'\u5C71', '\u9F8D',
		'\u05E9',
		'\u00DE', '\u00F0'
	];

	function generateDesignation(): string {
		const latin = Array.from({ length: 4 }, () =>
			LATIN[Math.floor(Math.random() * LATIN.length)]
		).join('');
		const suffix = NON_LATIN[Math.floor(Math.random() * NON_LATIN.length)];
		return `Ikshal-${latin}-${suffix}`;
	}

	// ---------------------------------------------------------------------------
	// Password strength (mirrors Rust implementation)
	// ---------------------------------------------------------------------------
	function strengthScore(pw: string): number {
		let score = 0;
		if (pw.length >= 12) score++;
		if (/[A-Z]/.test(pw)) score++;
		if (/[a-z]/.test(pw)) score++;
		if (/\d/.test(pw)) score++;
		if (/[^a-zA-Z0-9\s]/.test(pw)) score++;
		return score;
	}

	function strengthLabel(score: number): string {
		if (score === 0) return '';
		if (score <= 1) return 'Very weak';
		if (score <= 2) return 'Weak';
		if (score <= 3) return 'Fair';
		if (score <= 4) return 'Strong';
		return 'Very strong';
	}

	function strengthColor(score: number): string {
		if (score <= 1) return 'var(--error, #EF4444)';
		if (score <= 2) return 'var(--warning, #F59E0B)';
		if (score <= 3) return 'var(--accent-dim, #92610a)';
		if (score <= 4) return 'var(--success, #10B981)';
		return 'var(--success, #10B981)';
	}

	// ---------------------------------------------------------------------------
	// State
	// ---------------------------------------------------------------------------
	const BUBBLE_STYLES = ['icon', 'wave', 'spin', 'pulse', 'blink', 'rings', 'matrix', 'orbit', 'morph'] as const;

	let cryptoEnabled = $state(false);
	let step = $state(0);
	let designation = $state(generateDesignation());

	// Step 2 — Nickname
	let nickname = $state('');

	// Step 3 — Bubble style
	let bubbleStyle = $state<string>('icon');

	// Step 5 — Sample data
	let seedSampleData = $state(true);

	// Step 6 — Password
	let password = $state('');
	let passwordConfirm = $state('');
	let passwordStrength = $state(0);
	let passwordPolicyValid = $state(false);
	let passwordPolicyErrors = $state<string[]>([]);
	let policyDebounceTimer: ReturnType<typeof setTimeout> | null = null;

	// Step 7 — Duress password
	let duressPassword = $state('');
	let duressConfirm = $state('');
	let duressError = $state('');

	// Step 8 — Canary phrase
	let canaryPhrase = $state('');
	let canaryConfirm = $state('');

	// Step 9 — Keystroke enrollment
	let keystrokeSamples = $state<KeystrokeSampleDto[][]>([]);
	let currentKeystrokeInput = $state('');
	let currentKeyTimings = new Map<string, number>();
	let currentKeystrokes: KeystrokeSampleDto[] = [];

	// Computed step count depends on whether crypto is enabled
	let totalSteps = $derived(cryptoEnabled ? 9 : 5);

	// Whether the Next button should be enabled for the current step
	let canAdvance = $derived.by(() => {
		switch (step) {
			case 0: return true; // Welcome
			case 1: return true; // Nickname (optional)
			case 2: return true; // Bubble style (has default)
			case 3: return true; // Theme
			case 4: return true; // Sample data
			case 5: return passwordPolicyValid && password === passwordConfirm && password.length > 0;
			case 6: return true; // Duress — has skip
			case 7: return true; // Canary — has skip
			case 8: return true; // Keystroke — has skip
			default: return true;
		}
	});

	let isFinalStep = $derived(step === totalSteps - 1);

	// ---------------------------------------------------------------------------
	// Lifecycle
	// ---------------------------------------------------------------------------
	onMount(async () => {
		try {
			const result = await checkAuthState();
			cryptoEnabled = result.crypto_enabled;
		} catch {
			cryptoEnabled = false;
		}
	});

	// Debounced password policy validation
	$effect(() => {
		// Track password reactively
		const pw = password;
		passwordStrength = strengthScore(pw);

		if (policyDebounceTimer) clearTimeout(policyDebounceTimer);

		if (!pw) {
			passwordPolicyValid = false;
			passwordPolicyErrors = [];
			return;
		}

		policyDebounceTimer = setTimeout(async () => {
			try {
				const result = await validatePasswordPolicy(pw);
				passwordPolicyValid = result.valid;
				passwordPolicyErrors = result.errors;
			} catch {
				passwordPolicyValid = false;
				passwordPolicyErrors = ['Unable to validate password'];
			}
		}, 300);
	});

	// Duress password validation
	$effect(() => {
		if (duressPassword && duressPassword === password) {
			duressError = 'Duress password must differ from primary password';
		} else {
			duressError = '';
		}
	});

	// ---------------------------------------------------------------------------
	// Handlers
	// ---------------------------------------------------------------------------
	function handleBack() {
		if (step > 0) step--;
	}

	function handleNext() {
		if (!canAdvance && !canSkipCurrentStep()) return;

		if (isFinalStep) {
			handleComplete();
			return;
		}

		step++;
	}

	function canSkipCurrentStep(): boolean {
		return step === 6 || step === 7 || step === 8;
	}

	function handleSkip() {
		if (isFinalStep) {
			handleComplete();
			return;
		}
		step++;
	}

	async function handleComplete() {
		const data: OnboardingData = {
			nickname: nickname.trim() || null,
			bubble_style: bubbleStyle,
			seed_sample_data: seedSampleData,
			password: cryptoEnabled && password ? password : null,
			duress_password: cryptoEnabled && duressPassword.trim() ? duressPassword.trim() : null,
			canary_phrase: cryptoEnabled && canaryPhrase.trim() ? canaryPhrase.trim() : null,
			keystrokes: cryptoEnabled ? keystrokeSamples : []
		};

		try {
			await completeOnboarding(data);
			app.bubbleStyle = bubbleStyle;
			app.authState = 'ready';
		} catch (e) {
			console.error('Onboarding failed:', e);
		}
	}

	async function handleThemeToggle(name: 'dark' | 'light') {
		if (theme.current !== name) {
			try {
				await toggleTheme();
				applyTheme(name);
			} catch {
				applyTheme(name);
			}
		}
	}

	// ---------------------------------------------------------------------------
	// Keystroke enrollment handlers
	// ---------------------------------------------------------------------------
	function handleKeystrokeKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			submitKeystrokeSample();
			return;
		}
		currentKeyTimings.set(e.key, Date.now());
	}

	function handleKeystrokeKeyup(e: KeyboardEvent) {
		const pressTime = currentKeyTimings.get(e.key);
		if (pressTime) {
			currentKeystrokes.push({
				key: e.key,
				press_ms: pressTime,
				release_ms: Date.now()
			});
			currentKeyTimings.delete(e.key);
		}
	}

	function submitKeystrokeSample() {
		if (currentKeystrokeInput !== password) {
			currentKeystrokeInput = '';
			currentKeystrokes = [];
			currentKeyTimings.clear();
			return;
		}

		keystrokeSamples = [...keystrokeSamples, [...currentKeystrokes]];
		currentKeystrokeInput = '';
		currentKeystrokes = [];
		currentKeyTimings.clear();
	}
</script>

<div class="onboarding-overlay">
	<div class="wizard-card">
		<!-- Progress indicator -->
		<div class="progress-bar">
			<span class="progress-label">Step {step + 1} of {totalSteps}</span>
			<div class="progress-track">
				<div
					class="progress-fill"
					style="width: {((step + 1) / totalSteps) * 100}%"
				></div>
			</div>
		</div>

		<!-- Step content -->
		<div class="step-content">
			<!-- Step 1: Welcome -->
			{#if step === 0}
				<div class="step-welcome">
					<h1 class="wizard-title">Welcome to Sovereign GE</h1>
					<div class="designation-display">
						<span class="designation-label">Your designation</span>
						<span class="designation-value">{designation}</span>
					</div>
					<p class="description">
						Sovereign GE is your personal, sovereign computing environment.
						Everything runs locally on your machine — your data, your AI, your rules.
					</p>
					<p class="description muted">
						This wizard will help you personalize your experience in a few quick steps.
					</p>
				</div>

			<!-- Step 2: Nickname -->
			{:else if step === 1}
				<div class="step-nickname">
					<h2 class="step-title">Name your AI</h2>
					<p class="description">
						Give your AI assistant a nickname. This is how it will introduce itself.
					</p>
					<input
						type="text"
						class="text-input"
						placeholder='e.g. "Ike", "T-Nine", "B4"...'
						bind:value={nickname}
						maxlength="32"
					/>
					<p class="hint">You can change this later in settings. Leave blank to skip.</p>
				</div>

			<!-- Step 3: Bubble Style -->
			{:else if step === 2}
				<div class="step-bubble">
					<h2 class="step-title">Choose a bubble style</h2>
					<p class="description">
						Pick how your AI assistant appears on screen.
					</p>
					<div class="bubble-grid">
						{#each BUBBLE_STYLES as s}
							<button
								class="bubble-option"
								class:selected={bubbleStyle === s}
								onclick={() => (bubbleStyle = s)}
							>
								<BubblePreview style={s} size={80} />
								<span class="bubble-label">{s}</span>
							</button>
						{/each}
					</div>
				</div>

			<!-- Step 4: Theme -->
			{:else if step === 3}
				<div class="step-theme">
					<h2 class="step-title">Choose your theme</h2>
					<p class="description">
						Select a visual theme for the interface.
					</p>
					<div class="theme-options">
						<button
							class="theme-btn"
							class:selected={theme.current === 'dark'}
							onclick={() => handleThemeToggle('dark')}
						>
							<div class="theme-preview dark-preview">
								<div class="preview-bar"></div>
								<div class="preview-body">
									<div class="preview-line"></div>
									<div class="preview-line short"></div>
								</div>
							</div>
							<span class="theme-label">Dark</span>
						</button>
						<button
							class="theme-btn"
							class:selected={theme.current === 'light'}
							onclick={() => handleThemeToggle('light')}
						>
							<div class="theme-preview light-preview">
								<div class="preview-bar"></div>
								<div class="preview-body">
									<div class="preview-line"></div>
									<div class="preview-line short"></div>
								</div>
							</div>
							<span class="theme-label">Light</span>
						</button>
					</div>
				</div>

			<!-- Step 5: Sample Data -->
			{:else if step === 4}
				<div class="step-sample-data">
					<h2 class="step-title">Sample data</h2>
					<p class="description">
						Load a set of example documents, threads, and contacts to explore
						Sovereign GE's features right away.
					</p>
					<button
						class="toggle-btn"
						class:active={seedSampleData}
						onclick={() => (seedSampleData = !seedSampleData)}
					>
						<div class="toggle-track">
							<div class="toggle-thumb"></div>
						</div>
						<span class="toggle-label">
							{seedSampleData ? 'Enabled' : 'Disabled'}
						</span>
					</button>
					<p class="hint">
						{seedSampleData
							? 'Sample documents and contacts will be created on first launch.'
							: 'You will start with a clean, empty workspace.'}
					</p>
				</div>

			<!-- Step 6: Password (crypto only) -->
			{:else if step === 5}
				<div class="step-password">
					<h2 class="step-title">Set your password</h2>
					<p class="description">
						This password encrypts your local data. Choose something strong — there is no recovery mechanism.
					</p>
					<div class="field-group">
						<label class="field-label" for="pw-main">Password</label>
						<input
							id="pw-main"
							type="password"
							class="text-input"
							placeholder="Enter password"
							bind:value={password}
						/>
					</div>
					<div class="field-group">
						<label class="field-label" for="pw-confirm">Confirm password</label>
						<input
							id="pw-confirm"
							type="password"
							class="text-input"
							placeholder="Confirm password"
							bind:value={passwordConfirm}
						/>
					</div>

					<!-- Strength bar -->
					{#if password.length > 0}
						<div class="strength-section">
							<div class="strength-bar">
								{#each Array(5) as _, i}
									<div
										class="strength-segment"
										style="background: {i < passwordStrength
											? strengthColor(passwordStrength)
											: 'var(--bg-input, #1e1e26)'}"
									></div>
								{/each}
							</div>
							<span class="strength-label" style="color: {strengthColor(passwordStrength)}">
								{strengthLabel(passwordStrength)}
							</span>
						</div>
					{/if}

					<!-- Validation errors -->
					{#if passwordPolicyErrors.length > 0}
						<ul class="validation-errors">
							{#each passwordPolicyErrors as err}
								<li>{err}</li>
							{/each}
						</ul>
					{/if}

					{#if passwordConfirm && password !== passwordConfirm}
						<p class="error-text">Passwords do not match</p>
					{/if}
				</div>

			<!-- Step 7: Duress Password (crypto only) -->
			{:else if step === 6}
				<div class="step-duress">
					<h2 class="step-title">Duress password</h2>
					<p class="description">
						A secondary password that opens a decoy workspace. If you are ever
						forced to unlock your device, entering this password will show an
						innocuous environment instead of your real data.
					</p>
					<div class="field-group">
						<label class="field-label" for="duress-pw">Duress password</label>
						<input
							id="duress-pw"
							type="password"
							class="text-input"
							placeholder="Enter duress password"
							bind:value={duressPassword}
						/>
					</div>
					<div class="field-group">
						<label class="field-label" for="duress-confirm">Confirm</label>
						<input
							id="duress-confirm"
							type="password"
							class="text-input"
							placeholder="Confirm duress password"
							bind:value={duressConfirm}
						/>
					</div>

					{#if duressError}
						<p class="error-text">{duressError}</p>
					{/if}
					{#if duressConfirm && duressPassword !== duressConfirm}
						<p class="error-text">Passwords do not match</p>
					{/if}

					<button class="skip-link" onclick={handleSkip}>Skip this step</button>
				</div>

			<!-- Step 8: Canary Phrase (crypto only) -->
			{:else if step === 7}
				<div class="step-canary">
					<h2 class="step-title">Canary phrase</h2>
					<p class="description">
						A personal phrase displayed after login. If it ever changes or
						disappears, you will know your system has been tampered with.
					</p>
					<div class="field-group">
						<label class="field-label" for="canary-phrase">Canary phrase</label>
						<input
							id="canary-phrase"
							type="text"
							class="text-input"
							placeholder="e.g. The cat sleeps on warm roofs"
							bind:value={canaryPhrase}
						/>
					</div>
					<div class="field-group">
						<label class="field-label" for="canary-confirm">Confirm phrase</label>
						<input
							id="canary-confirm"
							type="text"
							class="text-input"
							placeholder="Re-type your phrase"
							bind:value={canaryConfirm}
						/>
					</div>

					{#if canaryPhrase && canaryPhrase.length < 4}
						<p class="error-text">Phrase must be at least 4 characters</p>
					{/if}
					{#if canaryConfirm && canaryPhrase !== canaryConfirm}
						<p class="error-text">Phrases do not match</p>
					{/if}

					<button class="skip-link" onclick={handleSkip}>Skip this step</button>
				</div>

			<!-- Step 9: Keystroke Enrollment (crypto only) -->
			{:else if step === 8}
				<div class="step-keystroke">
					<h2 class="step-title">Keystroke enrollment</h2>
					<p class="description">
						Type your password 5 times so we can learn your typing rhythm.
						This adds a biometric layer to your login — even if someone knows
						your password, they cannot replicate your cadence.
					</p>

					<div class="keystroke-counter">
						Sample {Math.min(keystrokeSamples.length + 1, 5)} of 5
					</div>

					{#if keystrokeSamples.length < 5}
						<input
							type="password"
							class="text-input"
							placeholder="Type your password"
							bind:value={currentKeystrokeInput}
							onkeydown={handleKeystrokeKeydown}
							onkeyup={handleKeystrokeKeyup}
						/>
						{#if currentKeystrokeInput && currentKeystrokeInput !== password.slice(0, currentKeystrokeInput.length)}
							<p class="error-text">Does not match your password</p>
						{/if}
						<button
							class="submit-sample-btn"
							onclick={submitKeystrokeSample}
							disabled={currentKeystrokeInput !== password}
						>
							Submit sample
						</button>
					{:else}
						<div class="keystroke-complete">
							All 5 samples collected. You are ready to proceed.
						</div>
					{/if}

					<div class="sample-dots">
						{#each Array(5) as _, i}
							<div
								class="sample-dot"
								class:filled={i < keystrokeSamples.length}
							></div>
						{/each}
					</div>

					<button class="skip-link" onclick={handleSkip}>Skip this step</button>
				</div>
			{/if}
		</div>

		<!-- Navigation -->
		<div class="nav-row">
			{#if step > 0}
				<button class="nav-btn back-btn" onclick={handleBack}>
					Back
				</button>
			{:else}
				<div></div>
			{/if}

			<button
				class="nav-btn next-btn"
				onclick={handleNext}
				disabled={!canAdvance && !canSkipCurrentStep()}
			>
				{#if isFinalStep}
					Get Started
				{:else}
					Next
				{/if}
			</button>
		</div>
	</div>
</div>

<style>
	/* ===================================================================
	   Overlay
	   =================================================================== */
	.onboarding-overlay {
		position: fixed;
		inset: 0;
		z-index: 1000;
		background: var(--bg-primary, #1a1a20);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	/* ===================================================================
	   Card
	   =================================================================== */
	.wizard-card {
		width: 560px;
		max-height: 90vh;
		display: flex;
		flex-direction: column;
		background: var(--bg-panel, #252530);
		border: 1px solid var(--border, #333340);
		border-radius: 16px;
		padding: 32px 40px;
		overflow-y: auto;
	}

	/* ===================================================================
	   Progress
	   =================================================================== */
	.progress-bar {
		margin-bottom: 28px;
	}

	.progress-label {
		display: block;
		font-size: 0.75rem;
		color: var(--text-muted, #666);
		margin-bottom: 8px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.progress-track {
		width: 100%;
		height: 4px;
		background: var(--bg-input, #1e1e26);
		border-radius: 2px;
		overflow: hidden;
	}

	.progress-fill {
		height: 100%;
		background: var(--accent, #F59E0B);
		border-radius: 2px;
		transition: width 0.3s ease;
	}

	/* ===================================================================
	   Step content area
	   =================================================================== */
	.step-content {
		flex: 1;
		min-height: 320px;
		display: flex;
		flex-direction: column;
	}

	.step-content > div {
		display: flex;
		flex-direction: column;
	}

	/* ===================================================================
	   Typography
	   =================================================================== */
	.wizard-title {
		font-size: 1.8rem;
		font-weight: 700;
		color: var(--accent, #F59E0B);
		margin: 0 0 24px 0;
		text-align: center;
	}

	.step-title {
		font-size: 1.3rem;
		font-weight: 600;
		color: var(--text-primary, #e0e0e0);
		margin: 0 0 8px 0;
	}

	.description {
		color: var(--text-secondary, #999);
		font-size: 0.9rem;
		line-height: 1.5;
		margin: 0 0 20px 0;
	}

	.description.muted {
		color: var(--text-muted, #666);
		font-size: 0.85rem;
	}

	.hint {
		color: var(--text-muted, #666);
		font-size: 0.8rem;
		margin: 8px 0 0 0;
	}

	/* ===================================================================
	   Designation (Step 1)
	   =================================================================== */
	.designation-display {
		text-align: center;
		margin-bottom: 24px;
		padding: 16px;
		background: var(--bg-input, #1e1e26);
		border: 1px solid var(--border, #333340);
		border-radius: 10px;
	}

	.designation-label {
		display: block;
		font-size: 0.75rem;
		color: var(--text-muted, #666);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		margin-bottom: 6px;
	}

	.designation-value {
		font-size: 1.4rem;
		font-weight: 600;
		color: var(--text-primary, #e0e0e0);
		font-family: 'Consolas', 'Fira Code', monospace;
		letter-spacing: 0.04em;
	}

	/* ===================================================================
	   Inputs
	   =================================================================== */
	.text-input {
		width: 100%;
		padding: 12px 16px;
		background: var(--bg-input, #1e1e26);
		border: 1px solid var(--border, #333340);
		border-radius: 8px;
		color: var(--text-primary, #e0e0e0);
		font-size: 0.95rem;
		outline: none;
		box-sizing: border-box;
		transition: border-color 0.15s;
	}

	.text-input:focus {
		border-color: var(--accent, #F59E0B);
	}

	.text-input::placeholder {
		color: var(--text-muted, #666);
	}

	.field-group {
		margin-bottom: 14px;
	}

	.field-label {
		display: block;
		font-size: 0.8rem;
		color: var(--text-secondary, #999);
		margin-bottom: 6px;
		font-weight: 500;
	}

	/* ===================================================================
	   Bubble grid (Step 3)
	   =================================================================== */
	.bubble-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 12px;
		margin-top: 4px;
	}

	.bubble-option {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 12px;
		background: var(--bg-input, #1e1e26);
		border: 2px solid var(--border, #333340);
		border-radius: 12px;
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s;
	}

	.bubble-option:hover {
		background: var(--bg-hover, #30303d);
	}

	.bubble-option.selected {
		border-color: var(--accent, #F59E0B);
		background: var(--bg-hover, #30303d);
	}

	.bubble-label {
		font-size: 0.75rem;
		color: var(--text-secondary, #999);
		text-transform: capitalize;
	}

	.bubble-option.selected .bubble-label {
		color: var(--accent, #F59E0B);
		font-weight: 600;
	}

	/* ===================================================================
	   Theme toggle (Step 4)
	   =================================================================== */
	.theme-options {
		display: flex;
		gap: 20px;
		justify-content: center;
		margin-top: 8px;
	}

	.theme-btn {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		padding: 16px;
		background: var(--bg-input, #1e1e26);
		border: 2px solid var(--border, #333340);
		border-radius: 12px;
		cursor: pointer;
		transition: border-color 0.15s;
		min-width: 140px;
	}

	.theme-btn:hover {
		background: var(--bg-hover, #30303d);
	}

	.theme-btn.selected {
		border-color: var(--accent, #F59E0B);
	}

	.theme-preview {
		width: 120px;
		height: 80px;
		border-radius: 8px;
		overflow: hidden;
		border: 1px solid var(--border, #333340);
	}

	.dark-preview {
		background: #1a1a20;
	}

	.dark-preview .preview-bar {
		height: 16px;
		background: #252530;
		border-bottom: 1px solid #333340;
	}

	.dark-preview .preview-body {
		padding: 8px;
	}

	.dark-preview .preview-line {
		height: 6px;
		background: #333340;
		border-radius: 3px;
		margin-bottom: 6px;
	}

	.dark-preview .preview-line.short {
		width: 60%;
	}

	.light-preview {
		background: #f5f5f0;
	}

	.light-preview .preview-bar {
		height: 16px;
		background: #ffffff;
		border-bottom: 1px solid #d0d0c0;
	}

	.light-preview .preview-body {
		padding: 8px;
	}

	.light-preview .preview-line {
		height: 6px;
		background: #d0d0c0;
		border-radius: 3px;
		margin-bottom: 6px;
	}

	.light-preview .preview-line.short {
		width: 60%;
	}

	.theme-label {
		font-size: 0.85rem;
		color: var(--text-secondary, #999);
		font-weight: 500;
	}

	.theme-btn.selected .theme-label {
		color: var(--accent, #F59E0B);
		font-weight: 600;
	}

	/* ===================================================================
	   Toggle (Step 5)
	   =================================================================== */
	.toggle-btn {
		display: flex;
		align-items: center;
		gap: 12px;
		background: none;
		border: none;
		cursor: pointer;
		padding: 8px 0;
	}

	.toggle-track {
		width: 48px;
		height: 26px;
		border-radius: 13px;
		background: var(--bg-input, #1e1e26);
		border: 1px solid var(--border, #333340);
		position: relative;
		transition: background 0.2s, border-color 0.2s;
	}

	.toggle-btn.active .toggle-track {
		background: var(--accent, #F59E0B);
		border-color: var(--accent, #F59E0B);
	}

	.toggle-thumb {
		position: absolute;
		top: 3px;
		left: 3px;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		background: var(--text-muted, #666);
		transition: transform 0.2s, background 0.2s;
	}

	.toggle-btn.active .toggle-thumb {
		transform: translateX(22px);
		background: #fff;
	}

	.toggle-label {
		font-size: 0.9rem;
		color: var(--text-primary, #e0e0e0);
		font-weight: 500;
	}

	/* ===================================================================
	   Strength bar (Step 6)
	   =================================================================== */
	.strength-section {
		display: flex;
		align-items: center;
		gap: 12px;
		margin-bottom: 12px;
	}

	.strength-bar {
		display: flex;
		gap: 4px;
		flex: 1;
	}

	.strength-segment {
		flex: 1;
		height: 6px;
		border-radius: 3px;
		transition: background 0.2s;
	}

	.strength-label {
		font-size: 0.75rem;
		font-weight: 600;
		white-space: nowrap;
	}

	.validation-errors {
		list-style: none;
		padding: 0;
		margin: 0 0 12px 0;
	}

	.validation-errors li {
		color: var(--error, #EF4444);
		font-size: 0.8rem;
		padding: 2px 0;
	}

	.validation-errors li::before {
		content: '\2022';
		margin-right: 6px;
	}

	.error-text {
		color: var(--error, #EF4444);
		font-size: 0.8rem;
		margin: 4px 0 8px 0;
	}

	/* ===================================================================
	   Skip link (Steps 7-9)
	   =================================================================== */
	.skip-link {
		background: none;
		border: none;
		color: var(--text-muted, #666);
		font-size: 0.8rem;
		cursor: pointer;
		text-decoration: underline;
		text-underline-offset: 3px;
		padding: 8px 0;
		margin-top: 12px;
		align-self: center;
		transition: color 0.15s;
	}

	.skip-link:hover {
		color: var(--text-secondary, #999);
	}

	/* ===================================================================
	   Keystroke enrollment (Step 9)
	   =================================================================== */
	.keystroke-counter {
		font-size: 1.1rem;
		font-weight: 600;
		color: var(--accent, #F59E0B);
		text-align: center;
		margin-bottom: 16px;
	}

	.submit-sample-btn {
		margin-top: 10px;
		padding: 10px 20px;
		background: var(--accent, #F59E0B);
		color: #000;
		border: none;
		border-radius: 8px;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
		align-self: center;
		transition: background 0.15s;
	}

	.submit-sample-btn:hover:not(:disabled) {
		background: var(--accent-hover, #D97706);
	}

	.submit-sample-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.sample-dots {
		display: flex;
		gap: 10px;
		justify-content: center;
		margin-top: 16px;
	}

	.sample-dot {
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: var(--bg-input, #1e1e26);
		border: 2px solid var(--border, #333340);
		transition: background 0.2s, border-color 0.2s;
	}

	.sample-dot.filled {
		background: var(--accent, #F59E0B);
		border-color: var(--accent, #F59E0B);
	}

	.keystroke-complete {
		text-align: center;
		color: var(--success, #10B981);
		font-size: 0.9rem;
		font-weight: 500;
		padding: 20px 0;
	}

	/* ===================================================================
	   Navigation
	   =================================================================== */
	.nav-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-top: 28px;
		padding-top: 20px;
		border-top: 1px solid var(--border, #333340);
	}

	.nav-btn {
		padding: 10px 24px;
		border-radius: 8px;
		font-size: 0.9rem;
		font-weight: 600;
		cursor: pointer;
		transition: background 0.15s, opacity 0.15s;
	}

	.back-btn {
		background: var(--bg-input, #1e1e26);
		color: var(--text-secondary, #999);
		border: 1px solid var(--border, #333340);
	}

	.back-btn:hover {
		background: var(--bg-hover, #30303d);
		color: var(--text-primary, #e0e0e0);
	}

	.next-btn {
		background: var(--accent, #F59E0B);
		color: #000;
		border: none;
	}

	.next-btn:hover:not(:disabled) {
		background: var(--accent-hover, #D97706);
	}

	.next-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
</style>
