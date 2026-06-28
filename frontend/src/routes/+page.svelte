<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { getStatus, closeBrowserCmd, setBrowserVisible, listPendingSuggestions, listPeerReviews, auditPeerReviews } from '$lib/api/commands';
	import { app } from '$lib/stores/app.svelte';
	import { setSuggestions, type LinkSuggestion } from '$lib/stores/suggestions.svelte';
	import { setPeerReviews, fromDto } from '$lib/stores/peerReviews.svelte';
	import { chat } from '$lib/stores/chat.svelte';
	import { panels } from '$lib/stores/documents.svelte';
	import { browser, openBrowser, closeBrowser } from '$lib/stores/browser.svelte';
	import DocumentPanel from '$lib/components/DocumentPanel.svelte';
	import BrowserPanel from '$lib/components/BrowserPanel.svelte';
	import SuggestionPanel from '$lib/components/SuggestionPanel.svelte';
	import PeerReviewPanel from '$lib/components/PeerReviewPanel.svelte';
	import VisionPanel from '$lib/components/VisionPanel.svelte';
	import { vision } from '$lib/stores/vision.svelte';
	import Canvas from '$lib/components/Canvas.svelte';

	let error = $state('');

	// Cleanup is assigned at the end of the async onMount body; Svelte
	// ignores a Promise returned from an async onMount, so teardown is
	// registered via onDestroy instead of `return () => …`.
	let cleanup: (() => void) | null = null;
	onDestroy(() => cleanup?.());

	onMount(async () => {
		try {
			const status = await getStatus();
			app.orchestratorAvailable = status.orchestrator_available;

			// Load any pending AI-suggested links
			const dtos = await listPendingSuggestions();
			setSuggestions(
				dtos.map((d): LinkSuggestion => ({
					id: d.id,
					fromDocId: d.from_doc_id,
					fromTitle: d.from_title,
					toDocId: d.to_doc_id,
					toTitle: d.to_title,
					relationType: d.relation_type,
					strength: d.strength,
					rationale: d.rationale,
					source: d.source
				}))
			);

			// Load pending peer-write reviews (p2p-no-per-doc-authz). Best-effort
			// audit first to fill verdicts when a model is loaded (errors when no
			// orchestrator — e.g. mobile w/o GGUF — so ignore), then list.
			try {
				await auditPeerReviews();
			} catch { /* no model / orchestrator — list unaudited */ }
			const reviews = await listPeerReviews();
			setPeerReviews(reviews.map(fromDto));
		} catch (e) {
			error = String(e);
		}

		// Ctrl+B shortcut to toggle browser
		function handleKeydown(e: KeyboardEvent) {
			if ((e.ctrlKey || e.metaKey) && e.key === 'b') {
				e.preventDefault();
				toggleBrowser();
			}
		}
		window.addEventListener('keydown', handleKeydown);
		cleanup = () => window.removeEventListener('keydown', handleKeydown);
	});

	async function toggleBrowser() {
		if (browser.isOpen) {
			closeBrowser();
			try { await closeBrowserCmd(); } catch { /* ignore */ }
		} else {
			openBrowser('https://duckduckgo.com');
		}
	}

	// Hide browser webview when overlays are open
	$effect(() => {
		const overlayOpen = app.settingsVisible || chat.visible || app.inboxVisible;
		if (browser.isOpen) {
			setBrowserVisible(!overlayOpen).catch(() => {});
		}
	});
</script>

<main>
	<div class="main-content">
		<div class="canvas-area">
			<!-- Spatial canvas -->
			<Canvas />

			<!-- Document panels (float above canvas) -->
			{#each panels as panel (panel.doc.id)}
				<DocumentPanel {panel} />
			{/each}
		</div>

		{#if browser.isOpen}
			<BrowserPanel />
		{/if}
	</div>

	<!-- AI-suggested links panel (floats near bubble) -->
	<SuggestionPanel />

	<!-- Peer-write review panel (p2p-no-per-doc-authz) -->
	<PeerReviewPanel />

	{#if vision.open}
		<VisionPanel />
	{/if}

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

	.main-content {
		flex: 1;
		display: flex;
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
