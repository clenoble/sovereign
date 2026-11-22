<script lang="ts">
	/** Existing-device side of the pairing flow (P3.2).
	 *
	 *  Shown when the user clicks "Pair a new device" in Settings →
	 *  Devices. Calls `generate_pair_qr`, which arms the P2P node with a
	 *  single-use pairing offer, and renders:
	 *    - the offer as a real QR image (plus a copy-paste fallback),
	 *    - the pairing code the user types on the new device,
	 *    - a countdown to expiry.
	 *
	 *  Since P3.1 the QR carries NO secrets — only a short-lived offer
	 *  (peer id + dial hints). The pairing code is the only secret, it
	 *  never appears in the QR, and it is proven online with at most
	 *  three attempts before the offer self-destructs. The panel reacts
	 *  live to `device-paired` / `pairing-failed` events via the pairing
	 *  store, and disarms the offer on close.
	 *
	 *  Pairing a device again (spec ch. 05, PR1 and PR2): a device set up
	 *  again comes back with a new peer id under its old name. When the
	 *  device that joined has a name a paired device already holds, the
	 *  backend holds a question by the time `device-paired` arrives; the
	 *  panel asks it: « Replace it » or « Keep both » (« Keep all » with
	 *  several, one of which she picks). Nothing is answered for her; closing
	 *  the panel answers « keep », as no answer does.
	 */
	import { onMount, onDestroy } from 'svelte';
	import QRCode from 'qrcode';
	import {
		generatePairQr,
		cancelPairing,
		sameNameQuestion,
		answerSameName,
		type GeneratePairQrResult,
		type PairedDevice,
		type SameNameQuestion
	} from '$lib/api/commands';
	import { pairing, clearPairingStatus } from '$lib/stores/pairing.svelte';
	import { answeredSyncStopped, onGestureNeedingTheNode } from '$lib/stores/syncStopped.svelte';

	let {
		onClose,
		onDevicesChanged
	}: {
		onClose: () => void;
		/** Called once her answer has removed a pairing, so the list beside
		 *  the panel can show it. */
		onDevicesChanged?: () => void;
	} = $props();

	let qr = $state<GeneratePairQrResult | null>(null);
	let qrImageUrl = $state('');
	let loading = $state(false);
	let error = $state('');
	let now = $state(Date.now());
	let copied = $state(false);
	let showPayload = $state(false);
	let timer: ReturnType<typeof setInterval> | null = null;
	/** Timestamp of this panel's generate, so stale store state from an
	 *  earlier pairing doesn't show as this session's result. */
	let armedAt = $state(0);

	let secondsRemaining = $derived(
		qr ? Math.max(0, Math.ceil((qr.expires_at - now) / 1000)) : 0
	);
	let isExpired = $derived(qr !== null && secondsRemaining <= 0);

	let justPaired = $derived(
		pairing.lastPaired !== null && new Date(pairing.lastPaired.at).getTime() >= armedAt
	);
	let failure = $derived(pairing.lastFailure);

	// ---- pairing a device again (PR2) ----
	/** The question held for the device that just joined, while unanswered. */
	let question = $state<SameNameQuestion | null>(null);
	/** Her answer, once given: what the panel then says. */
	let answered = $state<'replaced' | 'kept' | null>(null);
	/** True while the question is being asked for. */
	let checking = $state(false);
	/** True while her answer goes to the backend. */
	let answering = $state(false);
	/** The paired device « Replace it » replaces (the first one by default, as
	 *  on the desktop). */
	let chosen = $state('');
	let keepButton = $state<HTMLButtonElement | null>(null);
	/** The peer id the question was asked for (not reactive: the effect below
	 *  asks once per pairing). */
	let askedFor: string | null = null;

	let several = $derived(question !== null && question.candidates.length > 1);

	/** The day it was paired, on this device's clock, then its id shortened as
	 *  the desktop shows it. A time that does not parse is shown by its first
	 *  ten characters, as it is stored. */
	function detail(d: PairedDevice): string {
		const t = new Date(d.paired_at);
		let day: string;
		if (Number.isNaN(t.getTime())) {
			day = d.paired_at.slice(0, 10);
		} else {
			const m = String(t.getMonth() + 1).padStart(2, '0');
			const dd = String(t.getDate()).padStart(2, '0');
			day = `${t.getFullYear()}-${m}-${dd}`;
		}
		const id =
			d.peer_id.length > 18 ? `${d.peer_id.slice(0, 8)}…${d.peer_id.slice(-6)}` : d.peer_id;
		return day ? `paired ${day}, id ${id}` : `id ${id}`;
	}

	let questionText = $derived.by(() => {
		if (!question) return '';
		const name = question.new_device_name;
		if (question.candidates.length === 1) {
			return (
				`Paired with ${name}. This device is already paired with a device named ${name} ` +
				`(${detail(question.candidates[0])}). If the new one is that device, set up again, ` +
				`replace the earlier pairing. If they are two devices, keep both: a device whose ` +
				`pairing is replaced stops syncing with this device.`
			);
		}
		return (
			`Paired with ${name}. This device is already paired with ${question.candidates.length} ` +
			`devices named ${name}. If the new one is one of them, set up again, choose it and ` +
			`replace its earlier pairing. Otherwise keep all: a device whose pairing is replaced ` +
			`stops syncing with this device.`
		);
	});

	let answeredText = $derived.by(() => {
		const name = pairing.lastPaired?.deviceName ?? '';
		if (answered === 'replaced') return `Paired with ${name}. The earlier pairing of ${name} is removed.`;
		if (answered === 'kept') {
			const kept = several ? 'All are kept' : 'Both are kept';
			return `Paired with ${name}. ${kept}; you can forget one in Devices.`;
		}
		return '';
	});

	// Once a pairing is reported, ask the backend whether it holds a question
	// for that device. A build without sync has no such command: nothing is
	// asked then, and both pairings stay, as with no answer.
	$effect(() => {
		if (!justPaired || !pairing.lastPaired) return;
		const peer = pairing.lastPaired.peerId;
		if (askedFor === peer) return;
		askedFor = peer;
		question = null;
		answered = null;
		checking = true;
		sameNameQuestion()
			.then((q) => {
				if (q && q.new_peer_id === peer && q.candidates.length > 0 && askedFor === peer) {
					question = q;
					chosen = q.candidates[0].peer_id;
				}
			})
			.catch((e) => console.warn('same_name_question:', e))
			.finally(() => (checking = false));
	});

	// The keyboard starts on « Keep both », the answer that removes nothing.
	$effect(() => {
		if (question && !answered && keepButton) keepButton.focus();
	});

	async function answer(replace: string | null) {
		if (!question || answered || answering) return;
		answering = true;
		error = '';
		try {
			await answerSameName(replace);
			answered = replace === null ? 'kept' : 'replaced';
			if (replace !== null) onDevicesChanged?.();
		} catch (e) {
			error = String(e);
		}
		answering = false;
	}

	/** Closing the panel with the question unanswered answers « keep », as no
	 *  answer does: nothing is removed. */
	function keepIfUnanswered() {
		if (question && !answered && !answering) {
			answered = 'kept';
			answerSameName(null).catch(() => {});
		}
	}

	async function generate() {
		// C-SUP, DEL-0019: « Generate new QR » while sync is stopped.
		if (onGestureNeedingTheNode()) return;
		loading = true;
		error = '';
		copied = false;
		qrImageUrl = '';
		clearPairingStatus();
		try {
			qr = await generatePairQr();
			armedAt = Date.now();
			qrImageUrl = await QRCode.toDataURL(qr.qr_payload_b64, {
				errorCorrectionLevel: 'M',
				margin: 2,
				width: 280
			});
		} catch (e) {
			// C-SUP, DEL-0019: while sync is stopped, the stop's notice and its gesture.
			if (!answeredSyncStopped(e)) error = String(e);
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

	function handleClose() {
		keepIfUnanswered();
		onClose();
	}

	onMount(() => {
		generate();
		timer = setInterval(() => (now = Date.now()), 500);
	});

	onDestroy(() => {
		if (timer) clearInterval(timer);
		keepIfUnanswered();
		clearPairingStatus();
		// Disarm the offer so the QR on a closed panel can't be redeemed.
		// Skip when pairing just finished — the offer is already consumed.
		if (!pairing.lastPaired || new Date(pairing.lastPaired.at).getTime() < armedAt) {
			cancelPairing().catch(() => {});
		}
	});
</script>

<div class="pair-panel" role="dialog" aria-label="Pair a new device">
	<div class="header">
		<span class="title">Pair a new device</span>
		<button class="close" onclick={handleClose} aria-label="Close">&#x2715;</button>
	</div>

	{#if error}
		<p class="error">{error}</p>
	{/if}

	{#if justPaired && pairing.lastPaired && question && !answered}
		<div
			class="same-name"
			role="group"
			aria-labelledby="same-name-title"
			aria-describedby="same-name-text"
		>
			<p id="same-name-title" class="success-title">
				<strong>Is this {question.new_device_name} set up again?</strong>
			</p>
			<p id="same-name-text" class="same-name-text">{questionText}</p>
			{#if several}
				<fieldset class="candidates">
					<legend>Devices named {question.new_device_name}</legend>
					{#each question.candidates as c (c.peer_id)}
						<label class="candidate">
							<input
								type="radio"
								name="same-name-candidate"
								value={c.peer_id}
								bind:group={chosen}
							/>
							<span>{c.device_name}, {detail(c)}</span>
						</label>
					{/each}
				</fieldset>
			{/if}
			<div class="answer-actions">
				<button
					type="button"
					class="primary"
					disabled={answering || !chosen}
					onclick={() => answer(chosen)}
				>
					Replace it
				</button>
				<button
					type="button"
					class="secondary"
					bind:this={keepButton}
					disabled={answering}
					onclick={() => answer(null)}
				>
					{several ? 'Keep all' : 'Keep both'}
				</button>
			</div>
		</div>
	{:else if justPaired && pairing.lastPaired}
		<div class="success" role="status">
			<span class="success-icon">&#x2713;</span>
			<div>
				{#if answered}
					<p class="success-title">{answeredText}</p>
				{:else}
					<p class="success-title">
						Paired with <strong>{pairing.lastPaired.deviceName}</strong>
					</p>
				{/if}
				<p class="success-sub">
					The new device is syncing now. You can close this panel.
				</p>
			</div>
		</div>
		{#if !checking}
			<button class="primary" onclick={handleClose}>Done</button>
		{/if}
	{:else if loading}
		<div class="loading">Preparing pairing offer...</div>
	{:else if qr}
		<p class="lead">
			On the new device, choose <strong>"Pair with existing device"</strong>
			during onboarding, scan this QR, then type the pairing code.
		</p>

		{#if qrImageUrl}
			<div class="qr-wrap">
				<img class="qr-img" src={qrImageUrl} alt="Pairing QR code" />
			</div>
		{/if}

		<div class="field">
			<span class="label">Pairing code — type this on the new device</span>
			<span class="pin">{qr.pin}</span>
		</div>

		{#if failure}
			<div class="failure" class:dead={failure.offerDead}>
				{#if failure.offerDead}
					Pairing locked: too many wrong codes (or the offer expired).
					Generate a new QR to try again.
				{:else}
					Wrong code entered on the new device
					({pairing.attemptsFailed} of 3 attempts used). The code is
					case-insensitive; the dash is optional.
				{/if}
			</div>
		{/if}

		<div class="status" class:expired={isExpired || failure?.offerDead}>
			{#if failure?.offerDead}
				This QR is no longer valid.
			{:else if isExpired}
				This offer has expired. Generate a new one to continue.
			{:else}
				Expires in {secondsRemaining}s
			{/if}
		</div>

		{#if isExpired || failure?.offerDead}
			<button class="primary" onclick={generate}>Generate new QR</button>
		{/if}

		<details class="fallback" bind:open={showPayload}>
			<summary>No camera? Copy the offer as text</summary>
			<textarea class="payload" readonly rows="4" value={qr.qr_payload_b64}></textarea>
			<div class="actions">
				<button class="copy-btn" onclick={copyPayload}>
					{copied ? 'Copied!' : 'Copy offer'}
				</button>
			</div>
		</details>

		<p class="warning">
			The QR contains no secrets — only the pairing code matters. Don't
			share the code in writing; read it out or type it yourself. Three
			wrong attempts invalidate this QR.
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

	.qr-wrap {
		display: flex;
		justify-content: center;
	}

	.qr-img {
		width: 240px;
		height: 240px;
		border-radius: 8px;
		/* QR quiet zone is baked in via the `margin` option; white
		   background keeps it scannable on dark themes. */
		background: #fff;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		align-items: center;
	}

	.label {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-muted);
	}

	.pin {
		font-family: 'Consolas', 'Fira Code', monospace;
		font-size: 1.5rem;
		font-weight: 700;
		color: var(--accent, #F59E0B);
		letter-spacing: 0.08em;
	}

	.success {
		display: flex;
		gap: 12px;
		align-items: flex-start;
		padding: 12px;
		background: rgba(34, 197, 94, 0.08);
		border-left: 3px solid #22c55e;
		border-radius: 4px;
	}

	.success-icon {
		color: #22c55e;
		font-size: 1.4rem;
		font-weight: 700;
	}

	.success-title {
		margin: 0;
		font-size: 0.9rem;
		color: var(--text-primary);
	}

	.success-sub {
		margin: 4px 0 0 0;
		font-size: 0.78rem;
		color: var(--text-muted);
	}

	.same-name {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px;
		background: rgba(245, 158, 11, 0.08);
		border-left: 3px solid var(--warning, #f59e0b);
		border-radius: 4px;
	}

	.same-name-text {
		margin: 0;
		font-size: 0.82rem;
		color: var(--text-primary);
		line-height: 1.5;
	}

	.candidates {
		margin: 0;
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: 4px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.candidates legend {
		font-size: 0.75rem;
		color: var(--text-secondary);
		padding: 0 4px;
	}

	.candidate {
		display: flex;
		gap: 8px;
		align-items: center;
		font-size: 0.82rem;
		color: var(--text-primary);
		min-height: 24px;
		cursor: pointer;
	}

	.answer-actions {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 8px;
	}

	.secondary {
		background: var(--bg-hover);
		border: 1px solid var(--border);
		color: var(--text-primary);
		padding: 8px 16px;
		font-size: 0.85rem;
		font-weight: 600;
		border-radius: 4px;
		cursor: pointer;
	}

	.answer-actions button:focus-visible,
	.candidate input:focus-visible {
		outline: 2px solid var(--text-primary);
		outline-offset: 2px;
	}

	.answer-actions button:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.failure {
		font-size: 0.8rem;
		color: var(--warning, #f59e0b);
		padding: 8px 10px;
		background: rgba(245, 158, 11, 0.08);
		border-left: 3px solid var(--warning, #f59e0b);
		border-radius: 4px;
		line-height: 1.4;
	}
	.failure.dead {
		color: var(--error, #ef4444);
		background: rgba(239, 68, 68, 0.08);
		border-left-color: var(--error, #ef4444);
	}

	.fallback {
		font-size: 0.78rem;
		color: var(--text-muted);
	}
	.fallback summary {
		cursor: pointer;
	}
	.fallback summary:hover {
		color: var(--accent);
	}

	.payload {
		width: 100%;
		margin-top: 8px;
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
		text-align: center;
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
