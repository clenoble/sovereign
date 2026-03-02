<script lang="ts">
	import { onMount } from 'svelte';
	import { canvas, type CanvasState } from '$lib/stores/canvas';
	import { createThread as apiCreateThread, moveDocumentToThread, importFile } from '$lib/api/commands';
	import CanvasCard from './CanvasCard.svelte';
	import Minimap from './Minimap.svelte';

	let canvasEl: HTMLCanvasElement;
	let containerEl: HTMLDivElement;
	let ctx: CanvasRenderingContext2D | null = null;

	// Pan state
	let panning = false;
	let panStart = { x: 0, y: 0 };
	let panCameraStart = { x: 0, y: 0 };

	// File drag-and-drop
	let dragOver = $state(false);

	// Thread creation
	let showNewThread = $state(false);
	let newThreadName = $state('');

	onMount(() => {
		canvas.load();
		ctx = canvasEl.getContext('2d');
		resizeCanvas();
		const resizeObs = new ResizeObserver(resizeCanvas);
		resizeObs.observe(containerEl);
		return () => resizeObs.disconnect();
	});

	function resizeCanvas() {
		if (!canvasEl || !containerEl) return;
		canvasEl.width = containerEl.clientWidth;
		canvasEl.height = containerEl.clientHeight;
		drawBackground($canvas);
	}

	// Redraw background whenever canvas state changes
	$effect(() => {
		drawBackground($canvas);
	});

	function drawBackground(state: CanvasState) {
		if (!ctx || !canvasEl) return;
		const { camera, threads, documents, relationships, milestones } = state;
		const w = canvasEl.width;
		const h = canvasEl.height;
		ctx.clearRect(0, 0, w, h);
		ctx.save();
		ctx.translate(camera.panX, camera.panY);
		ctx.scale(camera.zoom, camera.zoom);

		// Draw thread lane backgrounds
		const laneHeight = 120;
		const threadOrder = new Map<string, number>();
		threads.forEach((t, i) => threadOrder.set(t.id, i));

		// Find x-extent of documents
		let maxX = 1000;
		for (const d of documents) {
			maxX = Math.max(maxX, d.spatial_x + 220);
		}

		for (let i = 0; i < threads.length; i++) {
			const y = i * laneHeight;
			// Alternating lane backgrounds
			ctx.fillStyle = i % 2 === 0 ? 'rgba(255,255,255,0.02)' : 'rgba(255,255,255,0.04)';
			ctx.fillRect(-100, y, maxX + 200, laneHeight);

			// Lane label
			ctx.fillStyle = 'rgba(255,255,255,0.3)';
			ctx.font = '13px -apple-system, sans-serif';
			ctx.textBaseline = 'middle';
			ctx.fillText(threads[i].name, 10, y + laneHeight / 2);

			// Lane separator line
			ctx.strokeStyle = 'rgba(255,255,255,0.08)';
			ctx.lineWidth = 1;
			ctx.beginPath();
			ctx.moveTo(-100, y + laneHeight);
			ctx.lineTo(maxX + 200, y + laneHeight);
			ctx.stroke();
		}

		// Draw relationship edges
		for (const rel of relationships) {
			const fromDoc = documents.find((d) => d.id === rel.from_doc_id);
			const toDoc = documents.find((d) => d.id === rel.to_doc_id);
			if (!fromDoc || !toDoc) continue;

			const fromX = fromDoc.spatial_x + 100;
			const fromY = fromDoc.spatial_y + 40;
			const toX = toDoc.spatial_x + 100;
			const toY = toDoc.spatial_y + 40;

			// Color by relationship type
			let color = 'rgba(100,180,255,0.4)';
			if (rel.relation_type === 'DerivedFrom') color = 'rgba(255,200,100,0.4)';
			else if (rel.relation_type === 'Contradicts') color = 'rgba(255,100,100,0.4)';
			else if (rel.relation_type === 'Supports') color = 'rgba(100,255,100,0.4)';

			ctx.strokeStyle = color;
			ctx.lineWidth = 1 + rel.strength * 2;
			ctx.beginPath();
			// Bezier curve
			const midX = (fromX + toX) / 2;
			const midY = (fromY + toY) / 2 - 30;
			ctx.moveTo(fromX, fromY);
			ctx.quadraticCurveTo(midX, midY, toX, toY);
			ctx.stroke();
		}

		// Draw milestone markers
		for (const ms of milestones) {
			const thread = threads.find((t) => t.id === ms.thread_id);
			if (!thread) continue;
			const laneIdx = threadOrder.get(ms.thread_id) ?? 0;
			const y = laneIdx * laneHeight;
			// Find x position based on timestamp (simple linear from min to max dates)
			const msTime = new Date(ms.timestamp).getTime();
			const x = 200 + ((msTime % 100000000) / 100000000) * maxX;

			ctx.fillStyle = 'rgba(255,215,0,0.6)';
			ctx.beginPath();
			ctx.moveTo(x, y + 5);
			ctx.lineTo(x + 6, y + 15);
			ctx.lineTo(x - 6, y + 15);
			ctx.closePath();
			ctx.fill();

			ctx.fillStyle = 'rgba(255,215,0,0.5)';
			ctx.font = '10px -apple-system, sans-serif';
			ctx.fillText(ms.title, x + 8, y + 14);
		}

		ctx.restore();
	}

	// Pan handlers
	function handleCanvasPointerDown(e: PointerEvent) {
		if (e.button !== 0) return;
		// Only pan if clicking on empty canvas
		if ((e.target as HTMLElement).closest('.canvas-card')) return;
		panning = true;
		panStart = { x: e.clientX, y: e.clientY };
		panCameraStart = { x: $canvas.camera.panX, y: $canvas.camera.panY };
		containerEl.setPointerCapture(e.pointerId);
	}

	function handleCanvasPointerMove(e: PointerEvent) {
		if (!panning) return;
		const dx = e.clientX - panStart.x;
		const dy = e.clientY - panStart.y;
		canvas.panBy(
			panCameraStart.x + dx - $canvas.camera.panX,
			panCameraStart.y + dy - $canvas.camera.panY
		);
	}

	function handleCanvasPointerUp(e: PointerEvent) {
		if (panning) {
			panning = false;
			containerEl.releasePointerCapture(e.pointerId);
		}
	}

	function handleWheel(e: WheelEvent) {
		e.preventDefault();
		canvas.zoomAt(e.clientX, e.clientY, e.deltaY);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'h' || e.key === 'H') {
			canvas.home();
		} else if (e.key === '+' || e.key === '=') {
			canvas.zoomAt(window.innerWidth / 2, window.innerHeight / 2, -100);
		} else if (e.key === '-') {
			canvas.zoomAt(window.innerWidth / 2, window.innerHeight / 2, 100);
		} else if (e.key === 'ArrowLeft') {
			canvas.panBy(50, 0);
		} else if (e.key === 'ArrowRight') {
			canvas.panBy(-50, 0);
		} else if (e.key === 'ArrowUp') {
			canvas.panBy(0, 50);
		} else if (e.key === 'ArrowDown') {
			canvas.panBy(0, -50);
		}
	}

	async function handleCreateThread() {
		const name = newThreadName.trim();
		if (!name) return;
		try {
			await apiCreateThread(name, '');
			newThreadName = '';
			showNewThread = false;
			canvas.refresh();
		} catch (e) {
			console.error('Failed to create thread:', e);
		}
	}

	async function handleDrop(e: DragEvent) {
		e.preventDefault();
		e.stopPropagation();
		dragOver = false;
		if (!e.dataTransfer?.files?.length) return;

		for (const file of e.dataTransfer.files) {
			try {
				// Tauri provides the path via webkitRelativePath or a custom property
				// For Tauri v2, we use the file path from the drag event
				const filePath = (file as any).path || file.name;
				await importFile(filePath);
			} catch (err) {
				console.error('Failed to import file:', err);
			}
		}
		canvas.refresh();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="canvas-container"
	bind:this={containerEl}
	onpointerdown={handleCanvasPointerDown}
	onpointermove={handleCanvasPointerMove}
	onpointerup={handleCanvasPointerUp}
	onwheel={handleWheel}
	ondragover={(e) => { e.preventDefault(); e.stopPropagation(); dragOver = true; }}
	ondragleave={(e) => { e.stopPropagation(); dragOver = false; }}
	ondrop={handleDrop}
>
	<!-- Background canvas layer -->
	<canvas class="bg-canvas" bind:this={canvasEl}></canvas>

	<!-- Card layer with CSS transform for pan/zoom -->
	<div
		class="card-layer"
		style="transform: translate({$canvas.camera.panX}px, {$canvas.camera.panY}px) scale({$canvas.camera.zoom});"
	>
		{#each $canvas.documents as doc (doc.id)}
			<CanvasCard
				{doc}
				isHovered={$canvas.hoveredCardId === doc.id}
				isSelected={$canvas.selectedCardId === doc.id}
				zoom={$canvas.camera.zoom}
			/>
		{/each}
	</div>

	<!-- Canvas toolbar -->
	<div class="canvas-toolbar">
		<button class="toolbar-btn" onclick={() => (showNewThread = !showNewThread)} title="New thread">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
				<line x1="8" y1="3" x2="8" y2="13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
				<line x1="3" y1="8" x2="13" y2="8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
			</svg>
		</button>
	</div>

	{#if showNewThread}
		<div class="new-thread-popup">
			<input
				type="text"
				placeholder="Thread name..."
				bind:value={newThreadName}
				onkeydown={(e) => { if (e.key === 'Enter') handleCreateThread(); if (e.key === 'Escape') showNewThread = false; }}
				autofocus
			/>
			<button onclick={handleCreateThread}>Create</button>
		</div>
	{/if}

	<!-- Minimap overlay -->
	<Minimap />

	<!-- Loading / empty state -->
	{#if !$canvas.loaded}
		<div class="canvas-status">Loading canvas...</div>
	{:else if $canvas.documents.length === 0}
		<div class="canvas-status">No documents yet. Create one via chat or search.</div>
	{/if}

	{#if dragOver}
		<div class="drop-overlay">
			<div class="drop-message">Drop files to import</div>
		</div>
	{/if}
</div>

<style>
	.canvas-container {
		position: relative;
		flex: 1;
		overflow: hidden;
		cursor: grab;
	}

	.canvas-container:active {
		cursor: grabbing;
	}

	.bg-canvas {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}

	.card-layer {
		position: absolute;
		top: 0;
		left: 0;
		transform-origin: 0 0;
	}

	.canvas-toolbar {
		position: absolute;
		bottom: 8px;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		align-items: center;
		gap: 4px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 4px 8px;
		z-index: 10;
	}

	.toolbar-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		padding: 4px 8px;
		border-radius: 4px;
		font-size: 0.85rem;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.toolbar-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.new-thread-popup {
		position: absolute;
		bottom: 52px;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		gap: 8px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 8px 12px;
		z-index: 10;
	}

	.new-thread-popup input {
		background: transparent;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-primary);
		padding: 4px 8px;
		font-size: 0.85rem;
		outline: none;
		width: 180px;
	}

	.new-thread-popup button {
		background: var(--accent);
		color: #fff;
		border: none;
		border-radius: 4px;
		padding: 4px 12px;
		font-size: 0.85rem;
		cursor: pointer;
	}

	.canvas-status {
		position: absolute;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		color: var(--text-muted);
		font-size: 0.9rem;
		pointer-events: none;
	}

	.drop-overlay {
		position: absolute;
		inset: 0;
		background: rgba(90, 159, 212, 0.15);
		border: 3px dashed #5a9fd4;
		border-radius: 8px;
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 50;
		pointer-events: none;
	}

	.drop-message {
		font-size: 1.2rem;
		font-weight: 600;
		color: #5a9fd4;
		background: var(--bg-panel);
		padding: 12px 24px;
		border-radius: 8px;
	}
</style>
