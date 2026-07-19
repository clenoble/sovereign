<script lang="ts">
	/** Surface 1 — Guardian Access Recovery setup (Settings → Recovery).
	 *
	 *  Owner-side panel over the real F1 commands: the 5-guardian roster
	 *  (list_guardians) and in-person enrollment (begin_guardian_enrollment).
	 *  Guardians each hold one Shamir share of a Recovery Key that wraps the
	 *  account secrets; enrolling all 5 "arms" recovery. If the owner ever
	 *  forgets their password, any 3 guardians can help them back in — they
	 *  restore access, never see the data.
	 *
	 *  Enrollment is in person, by QR + spoken code. Per the spec, each
	 *  owner↔guardian pair also agrees an out-of-band recognition proof
	 *  (a shared memory / object / private question) — Sovereign stores
	 *  nothing about it; it's how the guardian will know it's really you at
	 *  recovery time.
	 */
	import { onMount } from 'svelte';
	import QRCode from 'qrcode';
	import {
		recoverySetup,
		refreshRoster,
		beginEnrollment,
		clearOffer,
		enrolledCount,
		isArmed,
		GUARDIAN_TOTAL,
		GUARDIAN_THRESHOLD
	} from '$lib/stores/recoverySetup.svelte';

	let offerQrUrl = $state('');
	let rosterPoll: ReturnType<typeof setInterval> | null = null;

	onMount(() => {
		refreshRoster();
		return () => {
			if (rosterPoll) clearInterval(rosterPoll);
		};
	});

	// Render the armed offer's payload as a QR whenever it appears.
	$effect(() => {
		const offer = recoverySetup.offer;
		if (!offer) {
			offerQrUrl = '';
			return;
		}
		QRCode.toDataURL(offer.qr_payload_b64, { errorCorrectionLevel: 'M', margin: 2, scale: 5 })
			.then((url) => (offerQrUrl = url))
			.catch(() => (offerQrUrl = ''));
	});

	async function handleAddGuardian() {
		const ok = await beginEnrollment();
		if (ok) startRosterPoll();
	}

	// While an offer is open, poll the roster so the guardian appears once
	// they've scanned + the share is persisted (no event bridge in F1).
	function startRosterPoll() {
		if (rosterPoll) clearInterval(rosterPoll);
		rosterPoll = setInterval(refreshRoster, 3000);
	}

	function handleCloseOffer() {
		clearOffer();
		if (rosterPoll) {
			clearInterval(rosterPoll);
			rosterPoll = null;
		}
		refreshRoster();
	}

	function fmtWhen(iso: string | null): string {
		if (!iso) return '';
		const d = new Date(iso);
		return isNaN(d.getTime()) ? iso : d.toLocaleDateString();
	}
</script>

<div class="recovery-panel">
	{#if recoverySetup.loading}
		<div class="loading">Loading your recovery setup...</div>
	{:else}
		{#if recoverySetup.error}
			<p class="error">{recoverySetup.error}</p>
		{/if}

		<!-- What this is -->
		<div class="section">
			<span class="section-label">Account recovery</span>
			<p class="hint">
				Pick five people you trust as guardians. Each holds one encrypted
				share of a recovery key — never your data. If you ever forget your
				password, any {GUARDIAN_THRESHOLD} of your {GUARDIAN_TOTAL} guardians
				can help you back into your account. Your secrets stay yours as long
				as fewer than {GUARDIAN_THRESHOLD} of your {GUARDIAN_TOTAL} guardians
				collude.
			</p>
		</div>

		<!-- Status + roster -->
		<div class="section">
			<div class="roster-head">
				<span class="section-label">
					Guardians ({enrolledCount()}/{GUARDIAN_TOTAL})
				</span>
				<span class="chip {isArmed() ? 'armed' : 'setup'}">
					{isArmed() ? 'Recovery ready' : 'Setup incomplete'}
				</span>
			</div>
			{#if !isArmed()}
				<p class="hint">
					Recovery turns on once all {GUARDIAN_TOTAL} guardians are enrolled
					— until then you can't recover with guardians, so keep your
					password safe.
				</p>
			{/if}

			{#if recoverySetup.roster}
				<ul class="slot-list">
					{#each recoverySetup.roster.guardians as slot, i (i)}
						<li class="slot" class:filled={slot.enrolled}>
							<span class="slot-index">{i + 1}</span>
							<div class="slot-body">
								{#if slot.enrolled}
									<span class="slot-label">{slot.label ?? 'Guardian'}</span>
									<span class="slot-meta">
										enrolled{slot.enrolled_at ? ` ${fmtWhen(slot.enrolled_at)}` : ''}
									</span>
								{:else}
									<span class="slot-label empty">Empty slot</span>
									<span class="slot-meta">not enrolled yet</span>
								{/if}
							</div>
							<span class="slot-state">{slot.enrolled ? '✓' : ''}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</div>

		<!-- Enrollment -->
		{#if recoverySetup.offer}
			<div class="section enroll">
				<span class="section-label">Enroll a guardian — in person</span>
				<div class="offer">
					{#if offerQrUrl}
						<img class="qr" src={offerQrUrl} alt="Guardian enrollment QR code" />
					{/if}
					<div class="offer-body">
						<p>
							Have your guardian open Sovereign on their own device and scan
							this QR, or read them the code:
						</p>
						<p class="code">{recoverySetup.offer.code}</p>
						<p class="hint">
							Slot {recoverySetup.offer.enrolled_count + 1} of
							{recoverySetup.offer.total}.
						</p>
					</div>
				</div>
				<div class="proof-note">
					<strong>Agree a recognition proof, out loud.</strong> Decide together
					on something only the two of you know — a shared memory, a private
					question, an object — that isn't findable online. It's how this
					guardian will know it's really you if you ever recover. Sovereign
					never stores it; it lives only between you.
				</div>
				<button class="secondary-btn" onclick={handleCloseOffer}>Done</button>
			</div>
		{:else if !isArmed()}
			<div class="section">
				{#if recoverySetup.offerError}
					<p class="error">{recoverySetup.offerError}</p>
				{/if}
				<button
					class="primary-btn"
					disabled={recoverySetup.enrolling}
					onclick={handleAddGuardian}
				>
					{recoverySetup.enrolling ? 'Preparing…' : 'Add a guardian'}
				</button>
			</div>
		{:else}
			<div class="section">
				<p class="armed-note">
					✓ All {GUARDIAN_TOTAL} guardians enrolled — you can recover your
					account with any {GUARDIAN_THRESHOLD} of them.
				</p>
			</div>
		{/if}
	{/if}
</div>

<style>
	.recovery-panel {
		display: flex;
		flex-direction: column;
		gap: 24px;
	}

	.loading {
		color: var(--text-muted, #666);
		font-size: 0.9rem;
		padding: 20px 0;
	}

	.error {
		color: var(--error, #ef4444);
		font-size: 0.85rem;
		margin: 0;
	}

	.section {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.section-label {
		color: var(--text-primary, #eee);
		font-size: 0.9rem;
		font-weight: 600;
	}

	.hint {
		color: var(--text-muted, #666);
		font-size: 0.8rem;
		line-height: 1.5;
		margin: 0;
	}

	.roster-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.chip {
		display: inline-block;
		padding: 2px 10px;
		border-radius: 10px;
		font-size: 0.75rem;
		font-weight: 600;
		border: 1px solid var(--border, #2a2a35);
		color: var(--text-secondary, #999);
		background: var(--bg-input, #1e1e26);
	}
	.chip.armed {
		color: var(--success, #10b981);
		border-color: var(--success, #10b981);
	}
	.chip.setup {
		color: var(--warning, #f59e0b);
		border-color: var(--warning, #f59e0b);
	}

	.slot-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.slot {
		display: flex;
		align-items: center;
		gap: 12px;
		background: var(--bg-input, #1e1e26);
		border: 1px solid var(--border, #2a2a35);
		border-radius: 8px;
		padding: 8px 12px;
	}
	.slot.filled {
		border-color: var(--success, #10b981);
	}

	.slot-index {
		width: 22px;
		height: 22px;
		flex: none;
		border-radius: 50%;
		background: var(--bg-panel, #242430);
		color: var(--text-muted, #888);
		font-size: 0.78rem;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.slot-body {
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
		flex: 1;
	}

	.slot-label {
		color: var(--text-primary, #eee);
		font-size: 0.86rem;
	}
	.slot-label.empty {
		color: var(--text-muted, #666);
	}
	.slot-meta {
		color: var(--text-muted, #666);
		font-size: 0.74rem;
	}
	.slot-state {
		color: var(--success, #10b981);
		font-size: 0.9rem;
	}

	.offer {
		display: flex;
		gap: 16px;
		align-items: flex-start;
		background: var(--bg-input, #1e1e26);
		border: 1px solid var(--border, #2a2a35);
		border-radius: 8px;
		padding: 12px;
	}
	.qr {
		width: 148px;
		height: 148px;
		border-radius: 6px;
		background: #fff;
		flex: none;
	}
	.offer-body p {
		margin: 0 0 6px 0;
		color: var(--text-secondary, #999);
		font-size: 0.85rem;
	}
	.code {
		color: var(--accent, #7aa2f7) !important;
		font-size: 1.1rem;
		letter-spacing: 0.12em;
		font-family: monospace;
	}

	.proof-note {
		background: var(--bg-input, #1e1e26);
		border-left: 3px solid var(--accent, #7aa2f7);
		border-radius: 4px;
		padding: 10px 12px;
		color: var(--text-secondary, #999);
		font-size: 0.82rem;
		line-height: 1.5;
	}
	.proof-note strong {
		color: var(--text-primary, #eee);
	}

	.primary-btn {
		align-self: flex-start;
		background: var(--accent, #7aa2f7);
		color: var(--bg-primary, #111);
		border: none;
		border-radius: 6px;
		padding: 8px 16px;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
	}
	.primary-btn:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}

	.secondary-btn {
		align-self: flex-start;
		background: var(--bg-input, #1e1e26);
		color: var(--text-primary, #eee);
		border: 1px solid var(--border, #2a2a35);
		border-radius: 6px;
		padding: 6px 14px;
		font-size: 0.8rem;
		cursor: pointer;
	}

	.armed-note {
		color: var(--success, #10b981);
		font-size: 0.85rem;
		margin: 0;
	}
</style>
