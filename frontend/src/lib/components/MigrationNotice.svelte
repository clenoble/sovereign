<!--
	The one-time migration's notice (C-DOC adoption step 6, M2): documents were
	converted to the new format, each one's earlier state is in its History.
	Shown once, until dismissed. The text comes from the backend.
-->
<script lang="ts">
	import { migrationNotice, dismissMigrationNotice } from '$lib/stores/migrationNotice.svelte';
</script>

{#if migrationNotice.text}
	<div class="notice" role="status">
		<span>{migrationNotice.text}</span>
		<button class="ok" onclick={dismissMigrationNotice}>OK</button>
	</div>
{/if}

<style>
	.notice {
		position: fixed;
		left: 50%;
		bottom: 84px;
		transform: translateX(-50%);
		z-index: 340;
		display: flex;
		align-items: center;
		gap: 14px;
		max-width: min(560px, 92vw);
		padding: 10px 14px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
		color: var(--text-primary);
		font-size: 0.9rem;
	}
	.ok {
		background: var(--accent, #f97316);
		color: #fff;
		border: none;
		border-radius: 6px;
		padding: 6px 12px;
		cursor: pointer;
		font-weight: 600;
		flex-shrink: 0;
	}
</style>
