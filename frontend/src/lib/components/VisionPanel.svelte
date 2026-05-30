<script lang="ts">
	import { onMount } from 'svelte';
	import { vision, closeVisionPanel } from '$lib/stores/vision.svelte';
	import { visionFrameUrl, openVisionWindow, closeVisionWindow } from '$lib/api/vision';

	let frameTs = $state(Date.now());
	let durationS = $state(300);

	onMount(() => {
		// Refresh the camera frame ~5 fps while the panel is open.
		const t = setInterval(() => {
			frameTs = Date.now();
		}, 200);
		return () => clearInterval(t);
	});

	const remaining = $derived(Math.round(vision.windowRemaining));
</script>

<div class="vision-panel">
	<header>
		<span class="title">Jiminy sees</span>
		<button class="close" onclick={closeVisionPanel} title="Close">✕</button>
	</header>

	<div class="feed">
		{#if vision.cameraOk}
			<img src={visionFrameUrl(frameTs)} alt="Jiminy camera feed" />
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
			<button onclick={() => openVisionWindow(durationS)} disabled={!vision.cameraOk}>
				Look ({durationS}s)
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
		margin-left: auto;
		cursor: pointer;
	}
	.win {
		color: #66aadd;
	}
</style>
