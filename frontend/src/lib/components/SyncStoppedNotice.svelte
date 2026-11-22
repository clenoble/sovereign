<!--
	C-SUP SU2 (spec ch. 05, *The node's supervision*): the node ended without
	the session asking for it. The status line says « Sync stopped » while it
	lasts; the notice shows once per stop, and again in answer to a gesture
	that needs the node (DEL-0019), with the gesture « Restart sync » beside it
	(rule A). Why it stopped is in the log, not here.
-->
<script lang="ts">
	import {
		dismissSyncStoppedNotice,
		restartSync,
		syncStoppedNotice,
		syncStoppedStatus
	} from '$lib/stores/syncStopped.svelte';

	let notice = $derived(syncStoppedNotice());
	let status = $derived(syncStoppedStatus());
	let busy = $state(false);

	async function restart() {
		if (busy) return;
		busy = true;
		try {
			await restartSync();
		} finally {
			busy = false;
		}
	}
</script>

{#if notice}
	<div class="notice" role="alert">
		<div class="body">
			<strong>{notice.title}</strong>
			<span>{notice.text}</span>
		</div>
		<button class="restart" onclick={restart} disabled={busy}>{notice.gesture}</button>
		<button class="close" onclick={dismissSyncStoppedNotice}>Dismiss</button>
	</div>
{:else if status}
	<div class="status" role="status">{status}</div>
{/if}

<style>
	.notice {
		position: fixed;
		left: 50%;
		bottom: 84px;
		transform: translateX(-50%);
		z-index: 341;
		display: flex;
		align-items: center;
		gap: 12px;
		max-width: min(600px, 92vw);
		padding: 10px 14px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
		color: var(--text-primary);
		font-size: 0.9rem;
	}
	.body {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	button {
		border-radius: 6px;
		padding: 6px 12px;
		cursor: pointer;
		font-weight: 600;
		flex-shrink: 0;
	}
	.restart {
		background: var(--accent, #f97316);
		color: #fff;
		border: none;
	}
	.restart:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.close {
		background: transparent;
		color: var(--text-primary);
		border: 1px solid var(--border);
	}
	.status {
		position: fixed;
		right: 12px;
		top: 12px;
		z-index: 339;
		padding: 4px 10px;
		border-radius: 4px;
		background: #f59e0b;
		color: #1a1a1a;
		font-size: 0.8rem;
		font-weight: 600;
	}
</style>
