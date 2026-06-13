<script lang="ts">
	import { app } from '$lib/stores/app.svelte';
	import {
		getProfile,
		saveProfile,
		getConfig,
		getTrustEntries,
		resetTrustAction,
		resetTrustAll,
		getCommsConfig,
		saveCommsConfig,
		listPairedDevices,
		forgetPairedDevice,
		getLocalPeerId,
		triggerSyncNow,
		getP2pSettings,
		resolveSyncConflictKeepMine
	} from '$lib/api/commands';
	import type {
		UserProfileDto,
		AppConfigDto,
		SaveProfileDto,
		TrustEntryDto,
		CommsConfigDto,
		SaveCommsConfigDto,
		PairedDevice,
		P2pSettings
	} from '$lib/api/commands';
	import BubblePreview from './BubblePreview.svelte';
	import PairQrPanel from './PairQrPanel.svelte';
	import { focusTrap } from '$lib/actions/focusTrap';
	import { sync, clearError, dismissConflict } from '$lib/stores/sync.svelte';
	import { pairing } from '$lib/stores/pairing.svelte';
	import { vision, setWindowSeconds } from '$lib/stores/vision.svelte';

	type Tab = 'profile' | 'ai' | 'security' | 'trust' | 'comms' | 'devices' | 'vision';

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

	// Devices state (Phase 5)
	let devicesLoading = $state(false);
	let pairedDevices = $state<PairedDevice[]>([]);
	let localPeerId = $state('');
	let pairPanelOpen = $state(false);
	let syncing = $state(false);
	let p2pSettings = $state<P2pSettings | null>(null);
	let resolvingConflict = $state('');

	// Live-refresh the paired list when the P3.1 handshake completes
	// while the pairing panel is open.
	$effect(() => {
		if (pairing.lastPaired && activeTab === 'devices') {
			loadDevices();
		}
	});

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
		} else if (activeTab === 'devices') {
			loadDevices();
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

	async function loadDevices() {
		devicesLoading = true;
		error = '';
		try {
			const [devices, pid, settings] = await Promise.all([
				listPairedDevices(),
				getLocalPeerId(),
				getP2pSettings().catch(() => null)
			]);
			pairedDevices = devices;
			localPeerId = pid;
			p2pSettings = settings;
		} catch (e) {
			error = String(e);
		}
		devicesLoading = false;
	}

	async function handleKeepMine(docId: string) {
		resolvingConflict = docId;
		error = '';
		try {
			await resolveSyncConflictKeepMine(docId);
			dismissConflict(docId);
		} catch (e) {
			error = String(e);
		}
		resolvingConflict = '';
	}

	async function handleForgetDevice(peerId: string) {
		error = '';
		try {
			await forgetPairedDevice(peerId);
			await loadDevices();
		} catch (e) {
			error = String(e);
		}
	}

	async function handleSyncNow() {
		syncing = true;
		clearError();
		try {
			await triggerSyncNow();
		} catch (e) {
			error = String(e);
		}
		syncing = false;
	}

	function copyPeerId() {
		if (!localPeerId) return;
		navigator.clipboard.writeText(localPeerId).catch((e) =>
			console.warn('Copy failed:', e)
		);
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
	<div
		class="settings-panel"
		role="dialog"
		aria-modal="true"
		aria-label="Settings"
		onkeydown={handleKeydown}
		use:focusTrap={{
			active: app.settingsVisible,
			onEscape: close
		}}
	>
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
			<button
				class="tab"
				class:active={activeTab === 'devices'}
				onclick={() => (activeTab = 'devices')}
			>
				Devices
			</button>
			<button
				class="tab"
				class:active={activeTab === 'vision'}
				onclick={() => (activeTab = 'vision')}
			>
				Vision
			</button>
		</div>

		{#if error}
			<p class="error">{error}</p>
		{/if}

		<div class="panel-body">
			{#if activeTab === 'vision'}
				<!-- Vision Tab -->
				<div class="form-section">
					<label class="field-label" for="settings-vision-window">
						Vision window duration (seconds)
					</label>
					<input
						id="settings-vision-window"
						class="field-input"
						type="number"
						min="10"
						max="3600"
						step="30"
						value={vision.windowSeconds}
						oninput={(e) => setWindowSeconds(Number(e.currentTarget.value))}
					/>
					<p class="hint">
						How long a “Look” runs the scene-understanding VLM before it turns off
						(default 300s). Open the camera tile from the taskbar to use it. The
						camera source (webcam / robot) is chosen when launching the
						jiminy-vision service.
					</p>
				</div>
			{:else if loading}
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

			{:else if activeTab === 'devices'}
				<!-- Devices Tab (Phase 5: P2P sync) -->
				{#if devicesLoading}
					<div class="loading">Loading paired devices...</div>
				{:else}
					<!-- Sync status summary -->
					<div class="sync-summary">
						<div class="sync-row">
							<span class="sync-label">Status</span>
							{#if sync.inProgress.size > 0}
								<span class="sync-value">Syncing with {sync.inProgress.size} peer(s)</span>
							{:else if sync.lastError}
								<span class="sync-value error">Last sync failed</span>
							{:else if sync.lastSyncedAt}
								<span class="sync-value">
									Last synced {new Date(sync.lastSyncedAt).toLocaleTimeString()}
								</span>
							{:else}
								<span class="sync-value muted">Idle</span>
							{/if}
						</div>
						{#if sync.lastError}
							<p class="sync-error-text">{sync.lastError}</p>
						{/if}
						<div class="sync-actions">
							<button
								class="sync-now-btn"
								onclick={handleSyncNow}
								disabled={syncing}
							>
								{syncing ? 'Triggering...' : 'Sync now'}
							</button>
						</div>
					</div>

					<!-- Local PeerId (debug helper) -->
					<div class="form-section">
						<label class="field-label">This device's PeerId</label>
						{#if localPeerId}
							<div class="peer-id-row">
								<code class="peer-id">{localPeerId}</code>
								<button class="copy-btn" onclick={copyPeerId}>Copy</button>
							</div>
						{:else}
							<span class="readonly-value muted">
								Not available (P2P feature disabled or pre-login)
							</span>
						{/if}
					</div>

					<!-- Paired devices list -->
					<div class="form-section">
						<label class="field-label">Paired devices</label>
						{#if pairedDevices.length === 0}
							<div class="empty-state">
								<p>No paired devices yet.</p>
								<p class="muted">
									Pair another device (e.g. your phone) to keep documents,
									threads, and the PII vault in sync across both.
								</p>
							</div>
						{:else}
							<ul class="device-list">
								{#each pairedDevices as device (device.peer_id)}
									<li class="device-item">
										<div class="device-info">
											<span class="device-name">{device.device_name}</span>
											<span class="device-meta">
												Paired {new Date(device.paired_at).toLocaleDateString()}
											</span>
											<code class="device-peer-id">{device.peer_id}</code>
										</div>
										<button
											class="forget-btn"
											onclick={() => handleForgetDevice(device.peer_id)}
										>
											Forget
										</button>
									</li>
								{/each}
							</ul>
						{/if}
					</div>

					<!-- Pair-new affordance / panel -->
					{#if !pairPanelOpen}
						<button class="primary-btn" onclick={() => (pairPanelOpen = true)}>
							Pair a new device
						</button>
					{:else}
						<PairQrPanel onClose={() => { pairPanelOpen = false; loadDevices(); }} />
					{/if}

					<!-- P2P configuration (read-only; edited via config.toml) -->
					{#if p2pSettings}
						<div class="form-section">
							<label class="field-label">P2P configuration</label>
							<div class="p2p-config">
								<div class="p2p-row">
									<span class="p2p-key">Sync</span>
									<span class="p2p-val" class:on={p2pSettings.enabled && p2pSettings.running}>
										{#if !p2pSettings.available}
											Not included in this build
										{:else if !p2pSettings.enabled}
											Disabled
										{:else if p2pSettings.running}
											Enabled &amp; running
										{:else}
											Enabled (node not running)
										{/if}
									</span>
								</div>
								<div class="p2p-row">
									<span class="p2p-key">Device name</span>
									<span class="p2p-val">{p2pSettings.device_name || '—'}</span>
								</div>
								<div class="p2p-row">
									<span class="p2p-key">LAN discovery (mDNS)</span>
									<span class="p2p-val">{p2pSettings.enable_mdns ? 'On' : 'Off'}</span>
								</div>
								<div class="p2p-row">
									<span class="p2p-key">Wi-Fi only</span>
									<span class="p2p-val">{p2pSettings.wifi_only ? 'On' : 'Off'}</span>
								</div>
							</div>
							<p class="hint">
								These are read from config.toml ([p2p] section) at startup.
								In-app editing arrives with the config rework.
							</p>
						</div>
					{/if}

					<!-- Conflicts (if any) -->
					{#if sync.conflicts.length > 0}
						<div class="form-section">
							<label class="field-label">Sync conflicts</label>
							<p class="hint">
								Both devices edited these at the same moment. "Keep mine"
								pushes this device's version to your other devices.
							</p>
							<ul class="conflict-list">
								{#each sync.conflicts as c (c.docId)}
									<li class="conflict-item">
										<div class="conflict-info">
											<span class="conflict-doc">{c.docId}</span>
											<span class="conflict-desc">{c.description}</span>
										</div>
										<div class="conflict-actions">
											<button
												class="conflict-btn primary"
												disabled={resolvingConflict === c.docId}
												onclick={() => handleKeepMine(c.docId)}
											>
												{resolvingConflict === c.docId ? 'Pushing...' : 'Keep mine'}
											</button>
											<button
												class="conflict-btn"
												onclick={() => dismissConflict(c.docId)}
											>
												Dismiss
											</button>
										</div>
									</li>
								{/each}
							</ul>
							<p class="hint">
								To inspect first, open the document and check its version
								history. "Keep theirs" needs a remote-fetch step and is
								coming in a later release — until then, the newer edit wins
								automatically on the next sync.
							</p>
						</div>
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
		background: color-mix(in srgb, var(--success) 15%, transparent);
		color: var(--success);
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
		background: color-mix(in srgb, var(--success) 15%, transparent);
		border-color: var(--success);
		color: var(--success);
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
		color: var(--success);
		text-align: center;
		margin: 8px 0 0 0;
	}

	/* Devices tab (Phase 5) */
	.sync-summary {
		margin-bottom: 20px;
		padding: 12px;
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 8px;
	}

	.sync-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 6px;
	}

	.sync-label {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-muted);
	}

	.sync-value {
		font-size: 0.85rem;
		color: var(--text-primary);
	}

	.sync-value.muted {
		color: var(--text-muted);
	}

	.sync-value.error {
		color: var(--error, #ef4444);
	}

	.sync-error-text {
		font-size: 0.75rem;
		color: var(--error, #ef4444);
		margin: 4px 0 8px 0;
	}

	.sync-actions {
		display: flex;
		justify-content: flex-end;
		margin-top: 8px;
	}

	.sync-now-btn {
		background: var(--bg-hover);
		border: 1px solid var(--border);
		color: var(--text-secondary);
		padding: 4px 12px;
		font-size: 0.75rem;
		border-radius: 4px;
		cursor: pointer;
	}

	.sync-now-btn:hover {
		color: var(--accent);
		border-color: var(--accent);
	}

	.peer-id-row {
		display: flex;
		gap: 8px;
		align-items: center;
	}

	.peer-id {
		flex: 1;
		font-family: 'Consolas', 'Fira Code', monospace;
		font-size: 0.7rem;
		padding: 6px 8px;
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-primary);
		word-break: break-all;
	}

	.copy-btn {
		background: var(--bg-hover);
		border: 1px solid var(--border);
		color: var(--text-secondary);
		padding: 4px 10px;
		font-size: 0.75rem;
		border-radius: 4px;
		cursor: pointer;
		flex-shrink: 0;
	}

	.copy-btn:hover {
		color: var(--accent);
		border-color: var(--accent);
	}

	.device-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.device-item {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 10px 12px;
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 6px;
		gap: 8px;
	}

	.device-info {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
		flex: 1;
	}

	.device-name {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.device-meta {
		font-size: 0.7rem;
		color: var(--text-muted);
	}

	.device-peer-id {
		font-family: 'Consolas', 'Fira Code', monospace;
		font-size: 0.65rem;
		color: var(--text-muted);
		word-break: break-all;
	}

	.forget-btn {
		background: none;
		border: 1px solid var(--border);
		color: var(--text-secondary);
		padding: 4px 10px;
		font-size: 0.75rem;
		border-radius: 4px;
		cursor: pointer;
		flex-shrink: 0;
	}

	.forget-btn:hover {
		color: var(--error, #ef4444);
		border-color: var(--error, #ef4444);
	}

	.primary-btn {
		width: 100%;
		padding: 10px;
		background: var(--accent);
		border: none;
		color: var(--bg-primary);
		font-size: 0.85rem;
		font-weight: 600;
		border-radius: 6px;
		cursor: pointer;
		margin-top: 4px;
	}

	.primary-btn:hover {
		opacity: 0.92;
	}

	.conflict-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.conflict-item {
		padding: 8px 10px;
		background: var(--bg-input);
		border-left: 3px solid var(--warning, #f59e0b);
		border-radius: 4px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.conflict-doc {
		font-family: 'Consolas', 'Fira Code', monospace;
		font-size: 0.75rem;
		color: var(--text-primary);
	}

	.conflict-desc {
		font-size: 0.7rem;
		color: var(--text-muted);
	}

	.conflict-info {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.conflict-actions {
		display: flex;
		gap: 6px;
		margin-top: 6px;
	}

	.conflict-btn {
		background: var(--bg-hover);
		border: 1px solid var(--border);
		color: var(--text-secondary);
		padding: 4px 10px;
		font-size: 0.72rem;
		border-radius: 4px;
		cursor: pointer;
	}
	.conflict-btn:hover {
		color: var(--accent);
		border-color: var(--accent);
	}
	.conflict-btn.primary {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--bg-primary);
		font-weight: 600;
	}
	.conflict-btn:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.p2p-config {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 10px;
		background: var(--bg-input);
		border-radius: 4px;
	}

	.p2p-row {
		display: flex;
		justify-content: space-between;
		font-size: 0.78rem;
	}

	.p2p-key {
		color: var(--text-muted);
	}

	.p2p-val {
		color: var(--text-secondary);
	}
	.p2p-val.on {
		color: #22c55e;
	}

	.muted {
		color: var(--text-muted);
	}

	@media (max-width: 768px) {
		.settings-panel {
			width: 100vw;
			border-left: none;
			box-shadow: none;
		}
	}
</style>
