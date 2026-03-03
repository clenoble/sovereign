<script lang="ts">
	import { app } from '$lib/stores/app.svelte';
	import { getProfile, saveProfile, getConfig, getTrustEntries, resetTrustAction, resetTrustAll, getCommsConfig, saveCommsConfig } from '$lib/api/commands';
	import type { UserProfileDto, AppConfigDto, SaveProfileDto, TrustEntryDto, CommsConfigDto, SaveCommsConfigDto } from '$lib/api/commands';
	import BubblePreview from './BubblePreview.svelte';

	type Tab = 'profile' | 'ai' | 'security' | 'trust' | 'comms';

	const BUBBLE_STYLES = ['icon', 'wave', 'spin', 'pulse', 'blink', 'rings', 'matrix', 'orbit', 'morph'];

	let activeTab = $state<Tab>('profile');
	let loading = $state(false);
	let saving = $state(false);
	let error = $state('');

	// Profile state
	let displayName = $state('');
	let nickname = $state('');
	let designation = $state('');
	let bubbleStyle = $state('icon');

	// AI config state
	let aiModelDir = $state('');
	let aiRouterModel = $state('');
	let aiReasoningModel = $state('');
	let aiGpuLayers = $state(0);
	let aiCtxSize = $state(2048);
	let aiPromptFormat = $state('chatml');

	// Security config state
	let cryptoEnabled = $state(false);
	let keystrokeEnabled = $state(false);
	let maxLoginAttempts = $state(10);
	let lockoutSeconds = $state(300);

	// Trust state
	let trustEntries = $state<TrustEntryDto[]>([]);
	let trustLoading = $state(false);

	// Comms state
	let commsConfig = $state<CommsConfigDto | null>(null);
	let commsLoading = $state(false);
	let commsSaving = $state(false);
	let commsSaveStatus = $state('');
	let emailEnabled = $state(false);
	let emailImapHost = $state('');
	let emailImapPort = $state(993);
	let emailSmtpHost = $state('');
	let emailSmtpPort = $state(587);
	let emailUsername = $state('');
	let signalEnabled = $state(false);
	let signalPhone = $state('');

	$effect(() => {
		if (app.settingsVisible) {
			loadData();
		}
	});

	$effect(() => {
		if (activeTab === 'trust') {
			loadTrust();
		} else if (activeTab === 'comms') {
			loadComms();
		}
	});

	async function loadTrust() {
		trustLoading = true;
		error = '';
		try {
			trustEntries = await getTrustEntries();
		} catch (e) {
			error = String(e);
		}
		trustLoading = false;
	}

	async function loadComms() {
		commsLoading = true;
		error = '';
		try {
			const cfg = await getCommsConfig();
			commsConfig = cfg;
			emailEnabled = cfg.email_configured;
			emailImapHost = cfg.email_imap_host;
			emailImapPort = cfg.email_imap_port;
			emailSmtpHost = cfg.email_smtp_host;
			emailSmtpPort = cfg.email_smtp_port;
			emailUsername = cfg.email_username;
			signalEnabled = cfg.signal_configured;
			signalPhone = cfg.signal_phone;
		} catch (e) {
			error = String(e);
		}
		commsLoading = false;
	}

	async function handleResetTrust(action: string) {
		error = '';
		try {
			await resetTrustAction(action);
			await loadTrust();
		} catch (e) {
			error = String(e);
		}
	}

	async function handleResetAllTrust() {
		error = '';
		try {
			await resetTrustAll();
			await loadTrust();
		} catch (e) {
			error = String(e);
		}
	}

	async function handleSaveComms() {
		commsSaving = true;
		commsSaveStatus = '';
		error = '';
		try {
			const data: SaveCommsConfigDto = {};
			if (emailEnabled) {
				data.email_imap_host = emailImapHost;
				data.email_imap_port = emailImapPort;
				data.email_smtp_host = emailSmtpHost;
				data.email_smtp_port = emailSmtpPort;
				data.email_username = emailUsername;
			}
			if (signalEnabled) {
				data.signal_phone = signalPhone;
			}
			await saveCommsConfig(data);
			commsSaveStatus = 'Saved successfully';
			setTimeout(() => { commsSaveStatus = ''; }, 3000);
		} catch (e) {
			error = String(e);
		}
		commsSaving = false;
	}

	async function loadData() {
		loading = true;
		error = '';
		try {
			const [profile, config] = await Promise.all([getProfile(), getConfig()]);
			applyProfile(profile);
			applyConfig(config);
		} catch (e) {
			error = String(e);
		}
		loading = false;
	}

	function applyProfile(p: UserProfileDto) {
		displayName = p.display_name ?? '';
		nickname = p.nickname ?? '';
		designation = p.designation;
		bubbleStyle = p.bubble_style || 'icon';
	}

	function applyConfig(c: AppConfigDto) {
		aiModelDir = c.ai_model_dir;
		aiRouterModel = c.ai_router_model;
		aiReasoningModel = c.ai_reasoning_model;
		aiGpuLayers = c.ai_n_gpu_layers;
		aiCtxSize = c.ai_n_ctx;
		aiPromptFormat = c.ai_prompt_format;
		cryptoEnabled = c.crypto_enabled;
		keystrokeEnabled = c.crypto_keystroke_enabled;
		maxLoginAttempts = c.crypto_max_login_attempts;
		lockoutSeconds = c.crypto_lockout_seconds;
	}

	async function handleSaveProfile() {
		saving = true;
		error = '';
		try {
			const data: SaveProfileDto = {
				display_name: displayName || undefined,
				nickname: nickname || undefined,
				bubble_style: bubbleStyle
			};
			await saveProfile(data);
			app.bubbleStyle = bubbleStyle;
		} catch (e) {
			error = String(e);
		}
		saving = false;
	}

	function close() {
		app.settingsVisible = false;
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			close();
		}
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if app.settingsVisible}
	<div class="settings-backdrop" onclick={close} onkeydown={handleKeydown}></div>
	<div class="settings-panel" onkeydown={handleKeydown}>
		<!-- Header -->
		<div class="panel-header">
			<span class="panel-title">Settings</span>
			<button class="close-btn" onclick={close}>&#x2715;</button>
		</div>

		<!-- Tabs -->
		<div class="tab-bar">
			<button
				class="tab"
				class:active={activeTab === 'profile'}
				onclick={() => (activeTab = 'profile')}
			>
				Profile
			</button>
			<button
				class="tab"
				class:active={activeTab === 'ai'}
				onclick={() => (activeTab = 'ai')}
			>
				AI
			</button>
			<button
				class="tab"
				class:active={activeTab === 'security'}
				onclick={() => (activeTab = 'security')}
			>
				Security
			</button>
			<button
				class="tab"
				class:active={activeTab === 'trust'}
				onclick={() => (activeTab = 'trust')}
			>
				Trust
			</button>
			<button
				class="tab"
				class:active={activeTab === 'comms'}
				onclick={() => (activeTab = 'comms')}
			>
				Comms
			</button>
		</div>

		{#if error}
			<p class="error">{error}</p>
		{/if}

		<div class="panel-body">
			{#if loading}
				<div class="loading">Loading settings...</div>

			{:else if activeTab === 'profile'}
				<!-- Profile Tab -->
				<div class="form-section">
					<label class="field-label" for="settings-display-name">Display name</label>
					<input
						id="settings-display-name"
						class="field-input"
						type="text"
						bind:value={displayName}
						placeholder="Your display name"
					/>
				</div>

				<div class="form-section">
					<label class="field-label" for="settings-nickname">Nickname</label>
					<input
						id="settings-nickname"
						class="field-input"
						type="text"
						bind:value={nickname}
						placeholder="How the AI addresses you"
					/>
				</div>

				<div class="form-section">
					<label class="field-label">Designation</label>
					<span class="designation-value">{designation || 'Not set'}</span>
				</div>

				<div class="form-section">
					<label class="field-label">Bubble style</label>
					<div class="bubble-grid">
						{#each BUBBLE_STYLES as s}
							<button
								class="bubble-cell"
								class:selected={bubbleStyle === s}
								onclick={() => (bubbleStyle = s)}
								title={s}
							>
								<BubblePreview style={s} size={60} />
							</button>
						{/each}
					</div>
				</div>

				<button
					class="save-btn"
					onclick={handleSaveProfile}
					disabled={saving}
				>
					{saving ? 'Saving...' : 'Save'}
				</button>

			{:else if activeTab === 'ai'}
				<!-- AI Tab -->
				<div class="form-section">
					<label class="field-label">Model directory</label>
					<span class="readonly-value">{aiModelDir || '(not configured)'}</span>
				</div>

				<div class="form-section">
					<label class="field-label">Router model</label>
					<span class="readonly-value">{aiRouterModel || '(none)'}</span>
				</div>

				<div class="form-section">
					<label class="field-label">Reasoning model</label>
					<span class="readonly-value">{aiReasoningModel || '(none)'}</span>
				</div>

				<div class="form-section">
					<label class="field-label" for="settings-gpu-layers">GPU layers</label>
					<input
						id="settings-gpu-layers"
						class="field-input narrow"
						type="number"
						min="0"
						max="99"
						bind:value={aiGpuLayers}
					/>
				</div>

				<div class="form-section">
					<label class="field-label" for="settings-ctx-size">Context size</label>
					<input
						id="settings-ctx-size"
						class="field-input narrow"
						type="number"
						min="256"
						bind:value={aiCtxSize}
					/>
				</div>

				<div class="form-section">
					<label class="field-label" for="settings-prompt-format">Prompt format</label>
					<select
						id="settings-prompt-format"
						class="field-select"
						bind:value={aiPromptFormat}
					>
						<option value="chatml">ChatML</option>
						<option value="mistral">Mistral</option>
						<option value="llama3">Llama3</option>
					</select>
				</div>

				<p class="note">Changes take effect after restart</p>

			{:else if activeTab === 'security'}
				<!-- Security Tab -->
				{#if cryptoEnabled}
					<div class="form-section">
						<label class="field-label">Encryption</label>
						<span class="badge badge-enabled">Enabled</span>
					</div>

					<div class="form-section">
						<label class="field-label" for="settings-keystroke">Keystroke auth</label>
						<button
							id="settings-keystroke"
							class="toggle-btn"
							class:active={keystrokeEnabled}
							onclick={() => (keystrokeEnabled = !keystrokeEnabled)}
						>
							{keystrokeEnabled ? 'On' : 'Off'}
						</button>
					</div>

					<div class="form-section">
						<label class="field-label" for="settings-max-attempts">Max login attempts</label>
						<input
							id="settings-max-attempts"
							class="field-input narrow"
							type="number"
							min="1"
							max="100"
							bind:value={maxLoginAttempts}
						/>
					</div>

					<div class="form-section">
						<label class="field-label" for="settings-lockout">Lockout seconds</label>
						<input
							id="settings-lockout"
							class="field-input narrow"
							type="number"
							min="10"
							bind:value={lockoutSeconds}
						/>
					</div>

					<p class="note">Changes take effect after restart</p>
				{:else}
					<div class="form-section">
						<label class="field-label">Encryption</label>
						<span class="badge badge-disabled">Disabled</span>
					</div>
					<p class="note">Enable encryption during onboarding to configure security settings.</p>
				{/if}

			{:else if activeTab === 'trust'}
				<!-- Trust Tab -->
				{#if trustLoading}
					<div class="loading">Loading trust data...</div>
				{:else if trustEntries.length === 0}
					<div class="empty-state">
						<p>No trust data yet — the AI learns your preferences as you approve or reject actions.</p>
					</div>
				{:else}
					<div class="trust-table-wrap">
						<table class="trust-table">
							<thead>
								<tr>
									<th>Action</th>
									<th>Approvals</th>
									<th>Auto-approve</th>
									<th>Last Rejected</th>
									<th></th>
								</tr>
							</thead>
							<tbody>
								{#each trustEntries as entry}
									<tr>
										<td class="trust-action">{entry.action}</td>
										<td class="trust-count">{entry.approval_count}</td>
										<td>
											{#if entry.auto_approve}
												<span class="badge badge-enabled">yes</span>
											{:else}
												<span class="badge badge-disabled">no</span>
											{/if}
										</td>
										<td class="trust-date">
											{entry.last_rejected ? new Date(entry.last_rejected).toLocaleDateString() : '—'}
										</td>
										<td>
											<button class="reset-btn" onclick={() => handleResetTrust(entry.action)}>Reset</button>
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>

					<button class="reset-all-btn" onclick={handleResetAllTrust}>
						Reset All
					</button>
				{/if}

			{:else if activeTab === 'comms'}
				<!-- Comms Tab -->
				{#if commsLoading}
					<div class="loading">Loading comms config...</div>
				{:else if commsConfig && !commsConfig.comms_available}
					<div class="empty-state">
						<p>Communications feature not enabled.</p>
					</div>
				{:else}
					<!-- Email section -->
					<div class="comms-section">
						<div class="comms-section-header">
							<span class="comms-section-title">Email</span>
							<button
								class="toggle-btn"
								class:active={emailEnabled}
								onclick={() => (emailEnabled = !emailEnabled)}
							>
								{emailEnabled ? 'On' : 'Off'}
							</button>
						</div>

						{#if emailEnabled}
							<div class="form-section">
								<label class="field-label" for="settings-imap-host">IMAP host</label>
								<input
									id="settings-imap-host"
									class="field-input"
									type="text"
									bind:value={emailImapHost}
									placeholder="imap.example.com"
								/>
							</div>

							<div class="form-section">
								<label class="field-label" for="settings-imap-port">IMAP port</label>
								<input
									id="settings-imap-port"
									class="field-input narrow"
									type="number"
									min="1"
									max="65535"
									bind:value={emailImapPort}
								/>
							</div>

							<div class="form-section">
								<label class="field-label" for="settings-smtp-host">SMTP host</label>
								<input
									id="settings-smtp-host"
									class="field-input"
									type="text"
									bind:value={emailSmtpHost}
									placeholder="smtp.example.com"
								/>
							</div>

							<div class="form-section">
								<label class="field-label" for="settings-smtp-port">SMTP port</label>
								<input
									id="settings-smtp-port"
									class="field-input narrow"
									type="number"
									min="1"
									max="65535"
									bind:value={emailSmtpPort}
								/>
							</div>

							<div class="form-section">
								<label class="field-label" for="settings-email-user">Username</label>
								<input
									id="settings-email-user"
									class="field-input"
									type="text"
									bind:value={emailUsername}
									placeholder="user@example.com"
								/>
							</div>
						{/if}
					</div>

					<!-- Signal section -->
					<div class="comms-section">
						<div class="comms-section-header">
							<span class="comms-section-title">Signal</span>
							<button
								class="toggle-btn"
								class:active={signalEnabled}
								onclick={() => (signalEnabled = !signalEnabled)}
							>
								{signalEnabled ? 'On' : 'Off'}
							</button>
						</div>

						{#if signalEnabled}
							<div class="form-section">
								<label class="field-label" for="settings-signal-phone">Phone number</label>
								<input
									id="settings-signal-phone"
									class="field-input"
									type="text"
									bind:value={signalPhone}
									placeholder="+1234567890"
								/>
							</div>
						{/if}
					</div>

					<button
						class="save-btn"
						onclick={handleSaveComms}
						disabled={commsSaving}
					>
						{commsSaving ? 'Saving...' : 'Save'}
					</button>

					{#if commsSaveStatus}
						<p class="save-status">{commsSaveStatus}</p>
					{/if}
				{/if}
			{/if}
		</div>
	</div>
{/if}

<style>
	.settings-backdrop {
		position: fixed;
		inset: 0;
		z-index: 69;
		background: rgba(0, 0, 0, 0.3);
	}

	.settings-panel {
		position: fixed;
		top: 0;
		right: 0;
		width: 400px;
		height: 100vh;
		background: var(--bg-panel);
		border-left: 1px solid var(--border);
		box-shadow: -4px 0 24px rgba(0, 0, 0, 0.4);
		z-index: 70;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 18px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.panel-title {
		font-size: 1rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.9rem;
		padding: 2px 6px;
	}
	.close-btn:hover {
		color: var(--error);
	}

	/* Tabs */
	.tab-bar {
		display: flex;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.tab {
		flex: 1;
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--text-secondary);
		font-size: 0.85rem;
		padding: 10px 0;
		cursor: pointer;
		transition: color 0.15s, border-color 0.15s;
	}
	.tab:hover {
		color: var(--text-primary);
		background: var(--bg-hover);
	}
	.tab.active {
		color: var(--accent);
		border-bottom-color: var(--accent);
	}

	/* Body */
	.panel-body {
		flex: 1;
		overflow-y: auto;
		padding: 16px 18px;
	}

	.loading {
		color: var(--text-muted);
		font-size: 0.85rem;
		text-align: center;
		padding: 32px 0;
	}

	.error {
		color: var(--error);
		font-size: 0.8rem;
		padding: 8px 18px;
		margin: 0;
	}

	/* Form fields */
	.form-section {
		margin-bottom: 16px;
	}

	.field-label {
		display: block;
		font-size: 0.75rem;
		font-weight: 500;
		color: var(--text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin-bottom: 6px;
	}

	.field-input {
		width: 100%;
		padding: 8px 12px;
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-primary);
		font-size: 0.9rem;
		outline: none;
		box-sizing: border-box;
	}
	.field-input:focus {
		border-color: var(--accent);
	}
	.field-input.narrow {
		width: 120px;
	}

	.field-select {
		width: 100%;
		padding: 8px 12px;
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-primary);
		font-size: 0.9rem;
		outline: none;
		box-sizing: border-box;
		cursor: pointer;
	}
	.field-select:focus {
		border-color: var(--accent);
	}

	.readonly-value {
		display: block;
		font-size: 0.85rem;
		color: var(--text-muted);
		padding: 6px 0;
		word-break: break-all;
	}

	.designation-value {
		display: inline-block;
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--accent);
		padding: 4px 0;
	}

	/* Bubble grid */
	.bubble-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 8px;
	}

	.bubble-cell {
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg-secondary);
		border: 2px solid var(--border);
		border-radius: 8px;
		padding: 6px;
		cursor: pointer;
		transition: border-color 0.15s;
	}
	.bubble-cell:hover {
		border-color: var(--text-secondary);
	}
	.bubble-cell.selected {
		border-color: var(--accent);
		background: var(--bg-hover);
	}

	/* Save button */
	.save-btn {
		width: 100%;
		padding: 10px;
		background: var(--accent);
		color: #000;
		border: none;
		border-radius: 6px;
		font-size: 0.9rem;
		font-weight: 600;
		cursor: pointer;
		margin-top: 8px;
	}
	.save-btn:hover:not(:disabled) {
		background: var(--accent-hover);
	}
	.save-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* Badges */
	.badge {
		display: inline-block;
		font-size: 0.75rem;
		font-weight: 600;
		padding: 3px 10px;
		border-radius: 4px;
	}
	.badge-enabled {
		background: rgba(34, 197, 94, 0.15);
		color: #22c55e;
	}
	.badge-disabled {
		background: rgba(239, 68, 68, 0.15);
		color: var(--error);
	}

	/* Toggle button */
	.toggle-btn {
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		color: var(--text-secondary);
		font-size: 0.8rem;
		padding: 5px 16px;
		border-radius: 4px;
		cursor: pointer;
	}
	.toggle-btn.active {
		background: rgba(34, 197, 94, 0.15);
		border-color: #22c55e;
		color: #22c55e;
	}

	/* Note text */
	.note {
		font-size: 0.78rem;
		color: var(--text-muted);
		font-style: italic;
		margin: 16px 0 0 0;
	}

	/* Empty state */
	.empty-state {
		text-align: center;
		padding: 32px 0;
		color: var(--text-muted);
		font-size: 0.85rem;
	}
	.empty-state p {
		margin: 0;
	}

	/* Trust table */
	.trust-table-wrap {
		overflow-x: auto;
		margin-bottom: 16px;
	}

	.trust-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8rem;
	}

	.trust-table th {
		text-align: left;
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		padding: 6px 8px;
		border-bottom: 1px solid var(--border);
	}

	.trust-table td {
		padding: 8px 8px;
		border-bottom: 1px solid var(--border);
		color: var(--text-primary);
		vertical-align: middle;
	}

	.trust-action {
		font-weight: 500;
		word-break: break-all;
	}

	.trust-count {
		text-align: center;
	}

	.trust-date {
		font-size: 0.75rem;
		color: var(--text-muted);
	}

	.reset-btn {
		background: none;
		border: 1px solid var(--border);
		color: var(--text-secondary);
		font-size: 0.75rem;
		padding: 3px 10px;
		border-radius: 4px;
		cursor: pointer;
		transition: color 0.15s, border-color 0.15s;
	}
	.reset-btn:hover {
		color: var(--error);
		border-color: var(--error);
	}

	.reset-all-btn {
		width: 100%;
		padding: 8px;
		background: none;
		border: 1px solid var(--border);
		color: var(--text-secondary);
		font-size: 0.8rem;
		border-radius: 6px;
		cursor: pointer;
		margin-top: 4px;
		transition: color 0.15s, border-color 0.15s;
	}
	.reset-all-btn:hover {
		color: var(--error);
		border-color: var(--error);
	}

	/* Comms sections */
	.comms-section {
		margin-bottom: 20px;
	}

	.comms-section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 12px;
	}

	.comms-section-title {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.save-status {
		font-size: 0.78rem;
		color: #22c55e;
		text-align: center;
		margin: 8px 0 0 0;
	}
</style>
