<script lang="ts">
	import { onMount } from 'svelte';
	import { getStatus } from '$lib/api/commands';
	import { app } from '$lib/stores/app.svelte';
	import { panels } from '$lib/stores/documents.svelte';
	import DocumentPanel from '$lib/components/DocumentPanel.svelte';
	import Canvas from '$lib/components/Canvas.svelte';

	let error = $state('');

	onMount(async () => {
		try {
			const status = await getStatus();
			app.orchestratorAvailable = status.orchestrator_available;
		} catch (e) {
			error = String(e);
		}
	});
</script>

<main>
	<div class="canvas-area">
		<!-- Spatial canvas -->
		<Canvas />

		<!-- Document panels (float above canvas) -->
		{#each panels as panel (panel.doc.id)}
			<DocumentPanel {panel} />
		{/each}
	</div>

	{#if error}
		<p class="error">{error}</p>
	{/if}
</main>

<style>
	main {
		flex: 1;
		display: flex;
		flex-direction: column;
		padding-bottom: 44px; /* taskbar height */
	}

	.canvas-area {
		flex: 1;
		display: flex;
		position: relative;
	}

	.error {
		position: fixed;
		bottom: 52px;
		left: 50%;
		transform: translateX(-50%);
		color: var(--error);
		font-size: 0.85rem;
		background: var(--bg-panel);
		padding: 4px 12px;
		border-radius: 6px;
		border: 1px solid var(--border);
		z-index: 50;
	}
</style>
