<script lang="ts">
	import { onMount } from 'svelte';
	import {
		guardianStatus,
		listRecoveryRequests,
		approveRecovery,
		denyRecovery,
		enrollGuardian,
		type GuardianStatus,
		type RecoveryRequest
	} from './lib/api';

	let status = $state<GuardianStatus | null>(null);
	let requests = $state<RecoveryRequest[]>([]);
	let error = $state('');

	// Enrollment form
	let showEnroll = $state(false);
	let offer = $state('');
	let code = $state('');
	let label = $state('');
	let enrolling = $state(false);
	let enrollError = $state('');

	// Per-request action state
	let busy = $state('');

	function reqKey(r: RecoveryRequest): string {
		return `${r.for_user}:${r.epoch}`;
	}

	function shortTag(tag: string): string {
		return tag.length > 20 ? `${tag.slice(0, 10)}…${tag.slice(-6)}` : tag;
	}

	function fmtWhen(iso: string | null): string {
		if (!iso) return '';
		const d = new Date(iso);
		return isNaN(d.getTime()) ? iso : d.toLocaleString();
	}

	async function refresh() {
		try {
			status = await guardianStatus();
			requests = await listRecoveryRequests();
			error = '';
		} catch (e) {
			error = String(e);
		}
	}

	async function handleApprove(r: RecoveryRequest) {
		if (
			!confirm(
				`Approve recovery for ${shortTag(r.for_user)}?\n\nOnly do this if you have verified — in person or by voice — that this is really them, using the proof you agreed when you enrolled. A 72-hour window still runs before the share leaves.`
			)
		)
			return;
		busy = reqKey(r);
		try {
			await approveRecovery(r.for_user, r.epoch);
			await refresh();
		} catch (e) {
			error = String(e);
		}
		busy = '';
	}

	async function handleDeny(r: RecoveryRequest) {
		busy = reqKey(r);
		try {
			await denyRecovery(r.for_user, r.epoch);
			await refresh();
		} catch (e) {
			error = String(e);
		}
		busy = '';
	}

	async function handleEnroll() {
		if (!offer.trim() || !code.trim() || enrolling) return;
		enrolling = true;
		enrollError = '';
		try {
			await enrollGuardian(offer.trim(), code.trim(), label.trim() || 'A friend');
			offer = '';
			code = '';
			label = '';
			showEnroll = false;
			await refresh();
		} catch (e) {
			enrollError = String(e);
		}
		enrolling = false;
	}

	onMount(() => {
		refresh();
		const t = setInterval(refresh, 3000);
		return () => clearInterval(t);
	});
</script>

<main>
	<header>
		<h1>Sovereign Guardian</h1>
		<p class="tagline">
			You hold one share of a friend's recovery key. Three of five recover;
			your share alone reveals nothing.
		</p>
	</header>

	{#if error}
		<p class="error">{error}</p>
	{/if}

	<!-- Pending recovery requests — the heart of the app -->
	<section>
		<h2>Recovery requests</h2>
		{#if requests.length === 0}
			<p class="muted">
				No one is recovering right now. When a friend you guard forgets their
				password, their request appears here.
			</p>
		{:else}
			<ul class="req-list">
				{#each requests as r (reqKey(r))}
					<li class="req">
						<div class="req-head">
							<span class="who">{shortTag(r.for_user)}</span>
							<span class="epoch">epoch {r.epoch}</span>
						</div>
						{#if r.requested_at}
							<span class="req-when">requested {fmtWhen(r.requested_at)}</span>
						{/if}
						<p class="verify">
							<strong>Is this really them?</strong> Before you approve, reach
							this person in the real world — call or meet — and check the
							proof you both agreed when you enrolled (a shared memory, an
							object, a private question). If you can't confirm it's them,
							deny.
						</p>
						<div class="req-actions">
							<button
								class="approve"
								disabled={busy === reqKey(r)}
								onclick={() => handleApprove(r)}
							>
								Approve
							</button>
							<button
								class="deny"
								disabled={busy === reqKey(r)}
								onclick={() => handleDeny(r)}
							>
								Deny
							</button>
						</div>
						<p class="hint">
							Approving arms your share; a 72-hour window still runs before it
							releases, so the real owner can stop an impostor.
						</p>
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<!-- Who you guard for -->
	<section>
		<h2>You guard for</h2>
		{#if status && status.duties.length > 0}
			<ul class="duty-list">
				{#each status.duties as d (d.shard_id)}
					<li class="duty">
						<span class="duty-label">{d.owner_label}</span>
						<span class="duty-meta">
							{d.threshold}-of-{d.total} · enrolled {fmtWhen(d.enrolled_at)}
						</span>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="muted">
				You're not guarding for anyone yet. When a friend asks you to be their
				guardian, they'll show you an offer — enrol below.
			</p>
		{/if}

		{#if showEnroll}
			<div class="enroll">
				<label>
					<span>Offer (from your friend's screen)</span>
					<textarea rows="3" bind:value={offer} placeholder="paste the offer"></textarea>
				</label>
				<label>
					<span>Spoken code</span>
					<input bind:value={code} placeholder="the code they read aloud" />
				</label>
				<label>
					<span>Label (how they'll see you)</span>
					<input bind:value={label} placeholder="e.g. Alex" />
				</label>
				{#if enrollError}
					<p class="error">{enrollError}</p>
				{/if}
				<div class="enroll-actions">
					<button class="approve" disabled={enrolling} onclick={handleEnroll}>
						{enrolling ? 'Enrolling…' : 'Enrol as guardian'}
					</button>
					<button class="ghost" onclick={() => (showEnroll = false)}>Cancel</button>
				</div>
			</div>
		{:else}
			<button class="ghost" onclick={() => (showEnroll = true)}>Enrol for a friend</button>
		{/if}
	</section>

	{#if status}
		<footer>
			<span class="peer">This guardian: <code>{shortTag(status.peer_id)}</code></span>
		</footer>
	{/if}
</main>

<style>
	:global(body) {
		margin: 0;
		background: #14151a;
		color: #e6e6ea;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
	}

	main {
		max-width: 460px;
		margin: 0 auto;
		padding: 24px 20px 40px;
		display: flex;
		flex-direction: column;
		gap: 24px;
	}

	header h1 {
		font-size: 1.3rem;
		margin: 0 0 6px 0;
		color: #7aa2f7;
	}
	.tagline {
		margin: 0;
		color: #9aa0ad;
		font-size: 0.85rem;
		line-height: 1.5;
	}

	h2 {
		font-size: 0.95rem;
		margin: 0 0 10px 0;
	}

	.muted {
		color: #6b7280;
		font-size: 0.85rem;
		line-height: 1.5;
		margin: 0;
	}

	.error {
		color: #ef4444;
		font-size: 0.85rem;
		margin: 0;
	}

	.req-list,
	.duty-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.req {
		background: #1c1e26;
		border: 1px solid #4a3a1a;
		border-left: 3px solid #f59e0b;
		border-radius: 10px;
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.req-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 10px;
	}
	.who {
		font-weight: 600;
		font-family: monospace;
		font-size: 0.9rem;
	}
	.epoch {
		color: #6b7280;
		font-size: 0.75rem;
	}
	.req-when {
		color: #6b7280;
		font-size: 0.75rem;
	}
	.verify {
		margin: 0;
		font-size: 0.85rem;
		line-height: 1.55;
		color: #cbd0da;
	}
	.verify strong {
		color: #f2f3f5;
	}
	.req-actions {
		display: flex;
		gap: 10px;
		margin-top: 2px;
	}
	.hint {
		margin: 0;
		color: #6b7280;
		font-size: 0.76rem;
		line-height: 1.5;
	}

	button {
		border: none;
		border-radius: 7px;
		padding: 9px 16px;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
	}
	button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.approve {
		background: #10b981;
		color: #08130d;
		flex: 1;
	}
	.deny {
		background: #1c1e26;
		color: #ef4444;
		border: 1px solid #ef4444;
		flex: 1;
	}
	.ghost {
		background: #1c1e26;
		color: #cbd0da;
		border: 1px solid #2a2c37;
	}

	.duty {
		display: flex;
		flex-direction: column;
		gap: 2px;
		background: #1c1e26;
		border: 1px solid #2a2c37;
		border-radius: 8px;
		padding: 10px 12px;
	}
	.duty-label {
		font-size: 0.88rem;
	}
	.duty-meta {
		color: #6b7280;
		font-size: 0.74rem;
	}

	.enroll {
		margin-top: 12px;
		display: flex;
		flex-direction: column;
		gap: 10px;
		background: #1c1e26;
		border: 1px solid #2a2c37;
		border-radius: 8px;
		padding: 12px;
	}
	.enroll label {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 0.78rem;
		color: #9aa0ad;
	}
	.enroll input,
	.enroll textarea {
		background: #14151a;
		border: 1px solid #2a2c37;
		border-radius: 6px;
		color: #e6e6ea;
		padding: 8px 10px;
		font-size: 0.85rem;
		font-family: inherit;
		resize: vertical;
	}
	.enroll-actions {
		display: flex;
		gap: 10px;
	}

	footer {
		border-top: 1px solid #2a2c37;
		padding-top: 12px;
	}
	.peer {
		color: #6b7280;
		font-size: 0.74rem;
	}
	code {
		font-family: monospace;
	}
</style>
