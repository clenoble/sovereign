<script lang="ts">
	/** Existing-device side of the pairing flow.
	 *
	 *  Shown when the user clicks "Pair a new device" in Settings →
	 *  Devices. Calls `generate_pair_qr`, displays the base64url payload
	 *  as a copy-paste string + the 6-digit PIN, and counts down to
	 *  expiry. The QR is generated on demand — closing the panel
	 *  doesn't immediately invalidate it (the backend `pending_pairing`
	 *  RwLock holds it until the new device redeems it or the TTL
	 *  expires), but a fresh `Generate` overwrites the previous one.
	 *
	 *  v0.0.5 doesn't render an actual QR image yet; it shows the
	 *  base64url string for paste-into-mobile or for typing on a paper
	 *  laptop. Phase 5+ can wrap with `qrcode-svg` for a true QR.
	 */
	import { onMount, onDestroy } from 'svelte';
	import { generatePairQr, type GeneratePairQrResult } from '$lib/api/commands';

	let { onClose }: { onClose: () => void } = $props();

	let qr = $state<GeneratePairQrResult | null>(null);
	let loading = $state(false);
	let error = $state('');
	let now = $state(Date.now());
	let copied = $state(false);
	let timer: ReturnType<typeof setInterval> | null = null;

	let secondsRemaining = $derived(
		qr ? Math.max(0, Math.ceil((qr.expires_at - now) / 1000)) : 0
	);

	let isExpired = $derived(qr !== null && secondsRemaining <= 0);

	async function generate() {
		loading = true;
		error = '';
		copied = false;
		try {
			qr = await generatePairQr();
		} catch (e) {
			error = String(e);
		}
		loading = false;
	}

	async function copyPayload() {
		if (!qr) return;
		try {
			await navigator.clipboard.writeText(qr.qr_payload_b64);
			copied = true;
			setTimeout(() => (copied = false), 2000);
		} catch (e) {
			error = `Copy failed: ${e}`;
		}
	}

	onMount(() => {
		generate();
		timer = setInterval(() => (now = Date.now()), 500);
	});

	onDestroy(() => {
		if (timer) clearInterval(timer);
	});
</script>

<div class="pair-panel" role="dialog" aria-label="Pair a new device">
	<div class="header">
		<span class="title">Pair a new device</span>
		<button class="close" onclick={onClose} aria-label="Close">&#x2715;</button>
	</div>

	{#if error}
		<p class="error">{error}</p>
	{/if}

	{#if loading}
		<div class="loading">Generating pairing payload...</div>
	{:else if qr}
		<p class="lead">
			On the new device, choose <strong>"Pair with existing device"</strong>
			during onboarding, paste the payload below, then enter the PIN.
		</p>

		<div class="field">
			<span class="label">PIN</span>
			<span class="pin">{qr.pin}</span>
		</div>

		<div class="field">
			<span class="label">Pairing payload</span>
			<textarea class="payload" readonly rows="4" value={qr.qr_payload_b64}></textarea>
			<div class="actions">
				<button class="copy-btn" onclick={copyPayload}>
					{copied ? 'Copied!' : 'Copy payload'}
				</button>
			</div>
		</div>

		<div class="status" class:expired={isExpired}>
			{#if isExpired}
				This payload has expired. Generate a new one to continue.
			{:else}
				Expires in {secondsRemaining}s
			{/if}
		</div>

		{#if isExpired}
			<button class="primary" onclick={generate}>Generate new</button>
		{/if}

		<p class="warning">
			Anyone with both the payload and PIN can pair their device with
			your account. Share both verbally if possible — never together
			in writing.
		</p>
	{/if}
</div>

<style>
	.pair-panel {
		background: var(--bg-input, #1e1e26);
		border: 1px solid var(--border, #333340);
		border-radius: 8px;
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.header {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.title {
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.close {
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.9rem;
	}
	.close:hover {
		color: var(--error);
	}

	.lead {
		margin: 0;
		font-size: 0.85rem;
		color: var(--text-secondary);
		line-height: 1.5;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.label {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-muted);
	}

	.pin {
		font-family: 'Consolas', 'Fira Code', monospace;
		font-size: 1.6rem;
		font-weight: 700;
		color: var(--accent, #F59E0B);
		letter-spacing: 0.1em;
	}

	.payload {
		width: 100%;
		padding: 8px 10px;
		font-family: 'Consolas', 'Fira Code', monospace;
		font-size: 0.7rem;
		background: var(--bg-primary);
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-primary);
		resize: vertical;
		word-break: break-all;
		white-space: pre-wrap;
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		margin-top: 4px;
	}

	.copy-btn {
		background: var(--bg-hover);
		border: 1px solid var(--border);
		color: var(--text-secondary);
		padding: 4px 10px;
		font-size: 0.75rem;
		border-radius: 4px;
		cursor: pointer;
	}
	.copy-btn:hover {
		color: var(--accent);
		border-color: var(--accent);
	}

	.status {
		font-size: 0.75rem;
		color: var(--text-muted);
	}

	.status.expired {
		color: var(--error, #ef4444);
	}

	.primary {
		background: var(--accent);
		border: none;
		color: var(--bg-primary);
		padding: 8px 16px;
		font-size: 0.85rem;
		font-weight: 600;
		border-radius: 4px;
		cursor: pointer;
	}

	.warning {
		font-size: 0.7rem;
		color: var(--text-muted);
		font-style: italic;
		margin: 0;
		line-height: 1.4;
	}

	.error {
		color: var(--error, #ef4444);
		font-size: 0.8rem;
		margin: 0;
	}

	.loading {
		color: var(--text-muted);
		font-size: 0.85rem;
		text-align: center;
		padding: 16px 0;
	}
</style>
