<script lang="ts">
	/** Top-level mobile chrome.
	 *
	 *  Compose order (top to bottom):
	 *    1. LaneHeader  (sticky)
	 *    2. Canvas area (existing Canvas.svelte for now — Phase 2 swaps in MobileCanvas)
	 *    3. BottomSheet (chat AI; chassis-only in Phase 1)
	 *    4. Fab         (floating + above the taskbar)
	 *    5. MobileTaskbar
	 *
	 *  Existing desktop overlay panels (Search, ConfirmAction, ModelPanel,
	 *  InboxPanel, ContactPanel, PiiDashboardPanel, SettingsPanel,
	 *  SignupCapturePrompt, AutofillPrompt, ContextMenu) are still mounted
	 *  by the root +layout.svelte — this component just provides the mobile
	 *  body chrome.
	 */
	import { onMount } from 'svelte';
	import { app } from '$lib/stores/app.svelte';
	import { load as canvasLoad } from '$lib/stores/canvas.svelte';
	import { listPendingSuggestions, getStatus } from '$lib/api/commands';
	import { setSuggestions, type LinkSuggestion } from '$lib/stores/suggestions.svelte';
	import LaneHeader from './LaneHeader.svelte';
	import MobileCanvas from './MobileCanvas.svelte';
	import MobileTaskbar from './MobileTaskbar.svelte';
	import Fab from './Fab.svelte';
	import BottomSheet from './BottomSheet.svelte';

	let error = $state('');

	onMount(async () => {
		try {
			const status = await getStatus();
			app.orchestratorAvailable = status.orchestrator_available;
			await canvasLoad();

			const dtos = await listPendingSuggestions();
			setSuggestions(
				dtos.map(
					(d): LinkSuggestion => ({
						id: d.id,
						fromDocId: d.from_doc_id,
						fromTitle: d.from_title,
						toDocId: d.to_doc_id,
						toTitle: d.to_title,
						relationType: d.relation_type,
						strength: d.strength,
						rationale: d.rationale,
						source: d.source
					})
				)
			);
		} catch (e) {
			error = String(e);
		}
	});
</script>

<div class="mobile-shell">
	<LaneHeader />

	<main class="canvas-area" aria-label="Canvas">
		<MobileCanvas />
	</main>

	{#if error}
		<p class="error">{error}</p>
	{/if}

	<BottomSheet />
	<Fab />
	<MobileTaskbar />
</div>

<style>
	.mobile-shell {
		width: 100vw;
		height: 100vh;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		position: relative;
		background: var(--bg-primary, #1a1a20);
	}

	.canvas-area {
		flex: 1;
		position: relative;
		overflow: hidden;
		min-height: 0;
	}

	.error {
		position: absolute;
		left: 50%;
		bottom: 110px;
		transform: translateX(-50%);
		color: var(--error, #ef4444);
		background: var(--bg-panel, #22222a);
		padding: 6px 12px;
		border-radius: 6px;
		border: 1px solid var(--border, #333);
		font-size: 0.8rem;
		z-index: 80;
	}
</style>
