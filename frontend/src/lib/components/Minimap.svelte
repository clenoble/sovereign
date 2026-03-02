<script lang="ts">
	import { onMount } from 'svelte';
	import { canvas } from '$lib/stores/canvas';

	const MAP_W = 200;
	const MAP_H = 120;

	let minimapCanvas: HTMLCanvasElement;
	let ctx: CanvasRenderingContext2D | null = null;
	let visible = $state(true);

	onMount(() => {
		ctx = minimapCanvas.getContext('2d');
	});

	$effect(() => {
		drawMinimap($canvas);
	});

	function drawMinimap(state: typeof $canvas) {
		if (!ctx || !visible) return;
		ctx.clearRect(0, 0, MAP_W, MAP_H);
		const { documents, camera } = state;
		if (documents.length === 0) return;

		// Compute world bounds
		let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
		for (const d of documents) {
			minX = Math.min(minX, d.spatial_x);
			minY = Math.min(minY, d.spatial_y);
			maxX = Math.max(maxX, d.spatial_x + 200);
			maxY = Math.max(maxY, d.spatial_y + 80);
		}
		const worldW = maxX - minX + 100;
		const worldH = maxY - minY + 100;
		const scale = Math.min(MAP_W / worldW, MAP_H / worldH);

		// Draw cards as dots
		for (const d of documents) {
			const x = (d.spatial_x - minX + 50) * scale;
			const y = (d.spatial_y - minY + 50) * scale;
			ctx.fillStyle = d.is_owned ? '#4ea7e9' : '#f97316';
			ctx.fillRect(x, y, Math.max(3, 200 * scale), Math.max(2, 80 * scale));
		}

		// Draw viewport rectangle
		const vw = typeof window !== 'undefined' ? window.innerWidth : 1200;
		const vh = typeof window !== 'undefined' ? window.innerHeight - 44 : 700;
		const vpLeft = (-camera.panX / camera.zoom - minX + 50) * scale;
		const vpTop = (-camera.panY / camera.zoom - minY + 50) * scale;
		const vpW = (vw / camera.zoom) * scale;
		const vpH = (vh / camera.zoom) * scale;

		ctx.strokeStyle = 'rgba(255,255,255,0.6)';
		ctx.lineWidth = 1;
		ctx.strokeRect(vpLeft, vpTop, vpW, vpH);
	}

	function handleMinimapClick(e: MouseEvent) {
		const rect = minimapCanvas.getBoundingClientRect();
		const mx = e.clientX - rect.left;
		const my = e.clientY - rect.top;

		const { documents, camera } = $canvas;
		if (documents.length === 0) return;

		let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
		for (const d of documents) {
			minX = Math.min(minX, d.spatial_x);
			minY = Math.min(minY, d.spatial_y);
			maxX = Math.max(maxX, d.spatial_x + 200);
			maxY = Math.max(maxY, d.spatial_y + 80);
		}
		const worldW = maxX - minX + 100;
		const worldH = maxY - minY + 100;
		const scale = Math.min(MAP_W / worldW, MAP_H / worldH);

		// Convert minimap click to world coordinates
		const worldX = mx / scale + minX - 50;
		const worldY = my / scale + minY - 50;

		const vw = typeof window !== 'undefined' ? window.innerWidth : 1200;
		const vh = typeof window !== 'undefined' ? window.innerHeight - 44 : 700;

		canvas.panBy(
			-worldX * camera.zoom + vw / 2 - camera.panX,
			-worldY * camera.zoom + vh / 2 - camera.panY
		);
	}
</script>

{#if visible}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="minimap" onclick={handleMinimapClick}>
		<canvas bind:this={minimapCanvas} width={MAP_W} height={MAP_H}></canvas>
		<button class="minimap-toggle" onclick={(e) => { e.stopPropagation(); visible = false; }} title="Hide minimap">
			&times;
		</button>
	</div>
{:else}
	<button class="minimap-show" onclick={() => (visible = true)} title="Show minimap">
		<svg width="14" height="14" viewBox="0 0 14 14" fill="none">
			<rect x="1" y="1" width="12" height="12" rx="2" stroke="currentColor" stroke-width="1.5" />
			<rect x="3" y="3" width="4" height="4" fill="currentColor" opacity="0.4" />
		</svg>
	</button>
{/if}

<style>
	.minimap {
		position: absolute;
		top: 12px;
		right: 12px;
		width: 200px;
		height: 120px;
		background: rgba(0, 0, 0, 0.6);
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
		z-index: 10;
		cursor: crosshair;
	}

	.minimap canvas {
		display: block;
	}

	.minimap-toggle {
		position: absolute;
		top: 2px;
		right: 4px;
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.9rem;
		padding: 0;
		line-height: 1;
	}

	.minimap-toggle:hover {
		color: var(--text-primary);
	}

	.minimap-show {
		position: absolute;
		top: 12px;
		right: 12px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-secondary);
		cursor: pointer;
		padding: 6px;
		z-index: 10;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.minimap-show:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
</style>
