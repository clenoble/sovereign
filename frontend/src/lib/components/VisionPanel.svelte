<script lang="ts">
	import { onMount } from 'svelte';
	import { vision, closeVisionPanel, setWindowSeconds } from '$lib/stores/vision.svelte';
	import { fetchVisionFrameObjectUrl, openVisionWindow, closeVisionWindow } from '$lib/api/vision';

	// The camera frame is fetched WITH the sidecar bearer token (an <img src>
	// can't carry one) and shown via an object URL we rotate ~5 fps.
	let frameUrl = $state<string | null>(null);

	onMount(() => {
		let current: string | null = null;
		let cancelled = false;
		const refresh = async () => {
			if (!vision.cameraOk) return;
			try {
				const next = await fetchVisionFrameObjectUrl();
				if (cancelled) {
					URL.revokeObjectURL(next);
					return;
				}
				const prev = current;
				current = next;
				frameUrl = next;
				if (prev) URL.revokeObjectURL(prev);
			} catch {
				/* keep the last good frame */
			}
		};
		refresh();
		const t = setInterval(refresh, 200);
		return () => {
			cancelled = true;
			clearInterval(t);
			if (current) URL.revokeObjectURL(current);
		};
	});

	const remaining = $derived(Math.round(vision.windowRemaining));
</script>

<div class="vision-panel">
	<header>
		<span class="title">Jiminy sees</span>
		<button class="close" onclick={closeVisionPanel} title="Close">✕</button>
	</header>

	<div class="feed">
		{#if vision.cameraOk && frameUrl}
			<img src={frameUrl} alt="Jiminy camera feed" />
		{:else}
			<div class="no-cam">
				Camera off — start the vision service:
				<code>vision_service.py --camera webcam</code>
			</div>
		{/if}
		{#if vision.gesture}
			<span class="gesture-badge">{vision.gesture}</span>
		{/if}
	</div>

	{#if vision.scene}
		<p class="scene">“{vision.scene}”</p>
	{/if}

	<footer>
		{#if vision.windowActive}
			<span class="win">Understanding… {remaining}s</span>
			<button onclick={() => closeVisionWindow()}>Stop</button>
		{:else}
			<div class="dur">
				<button class="step" onclick={() => setWindowSeconds(vision.windowSeconds - 30)} title="Shorter window">−</button>
				<span>{vision.windowSeconds}s</span>
				<button class="step" onclick={() => setWindowSeconds(vision.windowSeconds + 30)} title="Longer window">+</button>
			</div>
			<button onclick={() => openVisionWindow(vision.windowSeconds)} disabled={!vision.cameraOk}>
				Look
			</button>
		{/if}
	</footer>
</div>

<style>
	.vision-panel {
		position: fixed;
		bottom: 64px;
		right: 16px;
		width: 280px;
		background: var(--panel-bg, #1b1b1f);
		color: var(--text-primary, #eaeaea);
		border: 1px solid var(--border, #333);
		border-radius: 10px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
		z-index: 50;
		overflow: hidden;
		font-size: 13px;
	}
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 10px;
		border-bottom: 1px solid var(--border, #333);
	}
	.title {
		font-weight: 600;
	}
	.close {
		background: none;
		border: none;
		color: inherit;
		cursor: pointer;
		font-size: 14px;
	}
	.feed {
		position: relative;
		aspect-ratio: 4 / 3;
		background: #000;
	}
	.feed img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.no-cam {
		padding: 16px;
		text-align: center;
		opacity: 0.7;
		font-size: 12px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.no-cam code {
		font-size: 11px;
		opacity: 0.85;
	}
	.gesture-badge {
		position: absolute;
		top: 8px;
		left: 8px;
		background: rgba(0, 0, 0, 0.6);
		padding: 2px 8px;
		border-radius: 12px;
		font-size: 12px;
		text-transform: capitalize;
	}
	.scene {
		margin: 0;
		padding: 8px 10px;
		font-style: italic;
		opacity: 0.9;
	}
	footer {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		border-top: 1px solid var(--border, #333);
	}
	footer button {
		cursor: pointer;
	}
	footer > button:last-child {
		margin-left: auto;
	}
	.dur {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.dur span {
		min-width: 40px;
		text-align: center;
		font-variant-numeric: tabular-nums;
	}
	.step {
		width: 22px;
		height: 22px;
		padding: 0;
		line-height: 1;
	}
	.win {
		color: #66aadd;
	}
</style>
