<!--
	The notice after a one-click delete, with Undo (C-DOC adoption step 5,
	S5-8; decided by Céline 2026-10-17). It disappears after the Undo window
	(10 s); the document then stays in Settings → Trash.
-->
<script lang="ts">
	import { refresh as canvasRefresh } from '$lib/stores/canvas.svelte';
	import { undo, undoDelete, UNDO_WINDOW_MS } from '$lib/stores/deleteUndo.svelte';

	// Hide the notice once its window has passed.
	$effect(() => {
		const n = undo.notice;
		if (!n) return;
		const left = Math.max(0, n.shownAt + UNDO_WINDOW_MS - Date.now());
		const timer = setTimeout(() => {
			if (undo.notice === n) undo.notice = null;
		}, left);
		return () => clearTimeout(timer);
	});

	async function onUndo() {
		if (await undoDelete()) {
			canvasRefresh().catch(() => {});
		}
	}
</script>

{#if undo.notice}
	<div class="toast" role="status">
		<span>{undo.notice.text}</span>
		<button class="undo" onclick={onUndo}>Undo</button>
	</div>
{:else if undo.error}
	<div class="toast error" role="alert">
		<span>{undo.error}</span>
		<button class="undo" onclick={() => (undo.error = null)} aria-label="Dismiss">×</button>
	</div>
{/if}

<style>
	.toast {
		position: fixed;
		left: 50%;
		bottom: 24px;
		transform: translateX(-50%);
		z-index: 350;
		display: flex;
		align-items: center;
		gap: 14px;
		max-width: 92vw;
		padding: 10px 14px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
		color: var(--text-primary);
		font-size: 0.9rem;
	}
	.toast.error {
		border-color: var(--error, #ef4444);
	}
	.undo {
		background: var(--accent, #f97316);
		color: #fff;
		border: none;
		border-radius: 6px;
		padding: 6px 12px;
		cursor: pointer;
		font-weight: 600;
	}
</style>
