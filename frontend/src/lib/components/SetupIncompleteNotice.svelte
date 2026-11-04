<!--
	The notice after an interrupted setup (v0.0.12 P5g): the account works, but the
	safety phrase, typing profile and sample documents may not have been saved.
	Shown once in the unlocked app, after the first successful login, until dismissed.
	Same shape as MigrationNotice.
-->
<script lang="ts">
	import {
		setupNotice,
		dismissSetupNotice,
		SETUP_INCOMPLETE_TEXT
	} from '$lib/stores/setupNotice.svelte';
</script>

{#if setupNotice.visible}
	<div class="notice" role="status">
		<span>{SETUP_INCOMPLETE_TEXT}</span>
		<button class="ok" onclick={dismissSetupNotice}>OK</button>
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
