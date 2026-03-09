<script lang="ts">
	import type { CanvasDocDto } from '$lib/api/commands';
	import { canvas, selectCard, setDragging, moveCard, snapToLane, hoverCard } from '$lib/stores/canvas.svelte';
	import { openById } from '$lib/stores/documents.svelte';
	import { app } from '$lib/stores/app.svelte';

	interface Props {
		doc: CanvasDocDto;
		isHovered: boolean;
		isSelected: boolean;
		zoom: number;
	}

	let { doc, isHovered, isSelected, zoom = 1 }: Props = $props();

	let dragging = false;
	let dragStart = { x: 0, y: 0 };
	let dragOriginal = { x: 0, y: 0 };
	const DEAD_ZONE = 3;
	let dragActivated = false;

	function handlePointerDown(e: PointerEvent) {
		if (e.button !== 0) return;
		dragging = true;
		dragActivated = false;
		dragStart = { x: e.clientX, y: e.clientY };
		dragOriginal = { x: doc.spatial_x, y: doc.spatial_y };
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		selectCard(doc.id);
		e.stopPropagation();
	}

	function handlePointerMove(e: PointerEvent) {
		if (!dragging) return;
		const dx = e.clientX - dragStart.x;
		const dy = e.clientY - dragStart.y;
		if (!dragActivated && Math.abs(dx) + Math.abs(dy) < DEAD_ZONE) return;
		dragActivated = true;
		setDragging(doc.id);
		// Only vertical drag — X stays locked to timeline position
		const worldDy = dy / canvas.camera.zoom;
		moveCard(doc.id, doc.spatial_x, dragOriginal.y + worldDy);
	}

	function handlePointerUp(e: PointerEvent) {
		if (dragActivated) {
			snapToLane(doc.id);
		}
		dragging = false;
		dragActivated = false;
		setDragging(null);
		(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
	}

	function handleDblClick() {
		openById(doc.id);
	}

	function handleContextMenu(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		app.contextMenu = {
			x: e.clientX,
			y: e.clientY,
			docId: doc.id,
			threadId: doc.thread_id
		};
	}

	function timeAgo(iso: string): string {
		const diff = Date.now() - new Date(iso).getTime();
		const mins = Math.floor(diff / 60000);
		if (mins < 1) return 'just now';
		if (mins < 60) return `${mins}m ago`;
		const hrs = Math.floor(mins / 60);
		if (hrs < 24) return `${hrs}h ago`;
		const days = Math.floor(hrs / 24);
		return `${days}d ago`;
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if zoom < 0.15}
	<!-- Heatmap mode: rendered on background canvas, nothing here -->
{:else if zoom < 0.3}
	<!-- LOD: dot only -->
	<div
		class="canvas-dot"
		class:owned={doc.is_owned}
		class:external={!doc.is_owned}
		style="left: {doc.spatial_x}px; top: {doc.spatial_y}px; z-index: {isSelected ? 100 : isHovered ? 50 : 1};"
		onpointerdown={handlePointerDown}
		onpointermove={handlePointerMove}
		onpointerup={handlePointerUp}
		oncontextmenu={handleContextMenu}
		onpointerenter={() => hoverCard(doc.id)}
		onpointerleave={() => hoverCard(null)}
	></div>
{:else if zoom < 0.6}
	<!-- LOD: title only -->
	<div
		class="canvas-card simplified"
		class:owned={doc.is_owned}
		class:external={!doc.is_owned}
		class:hovered={isHovered}
		class:selected={isSelected}
		style="left: {doc.spatial_x}px; top: {doc.spatial_y}px; z-index: {isSelected ? 100 : isHovered ? 50 : 1};"
		onpointerdown={handlePointerDown}
		onpointermove={handlePointerMove}
		onpointerup={handlePointerUp}
		ondblclick={handleDblClick}
		oncontextmenu={handleContextMenu}
		onpointerenter={() => hoverCard(doc.id)}
		onpointerleave={() => hoverCard(null)}
	>
		<div class="card-title">{doc.title}</div>
		{#if doc.reliability_score != null}
			<span
				class="reliability-badge"
				class:high={doc.reliability_score >= 3.5}
				class:medium={doc.reliability_score >= 2.0 && doc.reliability_score < 3.5}
				class:low={doc.reliability_score < 2.0}
			>{doc.reliability_score.toFixed(1)}</span>
		{/if}
	</div>
{:else}
	<!-- LOD: full card -->
	<div
		class="canvas-card"
		class:owned={doc.is_owned}
		class:external={!doc.is_owned}
		class:hovered={isHovered}
		class:selected={isSelected}
		style="left: {doc.spatial_x}px; top: {doc.spatial_y}px; z-index: {isSelected ? 100 : isHovered ? 50 : 1};"
		onpointerdown={handlePointerDown}
		onpointermove={handlePointerMove}
		onpointerup={handlePointerUp}
		ondblclick={handleDblClick}
		oncontextmenu={handleContextMenu}
		onpointerenter={() => hoverCard(doc.id)}
		onpointerleave={() => hoverCard(null)}
	>
		<div class="card-title">{doc.title}</div>
		<div class="card-meta">{timeAgo(doc.modified_at)}</div>
		{#if doc.reliability_score != null}
			<span
				class="reliability-badge"
				class:high={doc.reliability_score >= 3.5}
				class:medium={doc.reliability_score >= 2.0 && doc.reliability_score < 3.5}
				class:low={doc.reliability_score < 2.0}
			>{doc.reliability_score.toFixed(1)}</span>
		{/if}
	</div>
{/if}

<style>
	.canvas-card {
		position: absolute;
		width: 200px;
		height: 80px;
		border-radius: 8px;
		padding: 10px 12px;
		cursor: grab;
		user-select: none;
		display: flex;
		flex-direction: column;
		justify-content: space-between;
		background: var(--bg-panel);
		border: 2px solid var(--border);
		transition: box-shadow 0.15s;
		overflow: hidden;
	}

	.canvas-card:active {
		cursor: grabbing;
	}

	.owned {
		border-color: var(--prov-owned);
		background: var(--prov-owned-bg);
	}

	.external {
		border-color: var(--prov-external);
		background: var(--prov-external-bg);
		transform: skewX(-5deg);
		border-radius: 4px;
	}
	.external .card-title,
	.external .card-meta {
		transform: skewX(5deg);
	}

	.hovered {
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
		filter: brightness(1.1);
	}

	.selected {
		border-width: 3px;
		box-shadow: 0 0 0 2px var(--accent);
	}

	.card-title {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.card-meta {
		font-size: 0.7rem;
		color: var(--text-muted);
	}

	.canvas-dot {
		position: absolute;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		cursor: grab;
	}
	.canvas-dot.owned {
		background: var(--prov-owned);
	}
	.canvas-dot.external {
		background: var(--prov-external);
	}

	.simplified {
		height: auto;
		min-height: 40px;
		padding: 8px 10px;
	}

	.reliability-badge {
		position: absolute;
		top: 4px;
		right: 4px;
		font-size: 0.6rem;
		font-weight: 700;
		padding: 1px 5px;
		border-radius: 8px;
		line-height: 1.4;
	}
	.reliability-badge.high {
		color: var(--reliability-high);
		background: var(--reliability-high-bg);
	}
	.reliability-badge.medium {
		color: var(--reliability-medium);
		background: var(--reliability-medium-bg);
	}
	.reliability-badge.low {
		color: var(--reliability-low);
		background: var(--reliability-low-bg);
	}
	/* Un-skew badge for external cards */
	.external .reliability-badge {
		transform: skewX(5deg);
	}
</style>
