<script lang="ts">
	import type { CanvasDocDto } from '$lib/api/commands';
	import { canvas, selectCard, setDragging, moveCard, snapToLane, hoverCard, expandDeck, MAX_VISUAL_ZOOM, DECK_PEEK, type DeckRole } from '$lib/stores/canvas.svelte';
	import { openById } from '$lib/stores/documents.svelte';
	import { app } from '$lib/stores/app.svelte';

	interface Props {
		doc: CanvasDocDto;
		role?: DeckRole;
		isHovered: boolean;
		isSelected: boolean;
		zoom: number;
	}

	let { doc, role, isHovered, isSelected, zoom = 1 }: Props = $props();

	// Default role for a lone card (no deck): it is its own front.
	const r = $derived<DeckRole>(
		role ?? { count: 1, peekIndex: 0, isFront: true, hidden: false, frontId: doc.id }
	);

	// Peeking cards sit up-left behind the front card. The offset is in WORLD
	// units (not screen px), so it scales with zoom and stays a constant fraction
	// of the lane — this is what keeps the fan inside the lane's top margin at
	// EVERY zoom (containment invariant: (DECK_VISIBLE−1)·DECK_PEEK ≤ top slack).
	// The front card (peekIndex 0) stays at its true timeline position.
	const deckShift = $derived(r.peekIndex > 0 ? r.peekIndex * DECK_PEEK : 0);
	const lx = $derived(doc.spatial_x - deckShift);
	const ly = $derived(doc.spatial_y - deckShift);
	// Front sits above its peeks; hover/selection still win.
	const zBase = $derived(isSelected ? 100 : isHovered ? 50 : 10 - r.peekIndex);
	const isPeek = $derived(!r.isFront);
	// Badge on every multi-card deck: gives each stack a count AND its expand
	// handle (a numberless stack couldn't be counted or fanned open).
	const showCount = $derived(r.isFront && r.count > 1);

	// Counter-scale once zoom exceeds MAX_VISUAL_ZOOM so the card stops
	// growing visually. Parent layer is scaled by `zoom`; we apply
	// `MAX_VISUAL_ZOOM / zoom` here to clamp effective size at zoom = 1.5.
	const cardScale = $derived(zoom > MAX_VISUAL_ZOOM ? MAX_VISUAL_ZOOM / zoom : 1);
	const cardTransform = $derived(
		cardScale === 1 ? '' : `transform: scale(${cardScale}); transform-origin: top left;`
	);

	// Shared absolute-position style for every LOD tier (true pos minus peek shift).
	const posStyle = $derived(`left: ${lx}px; top: ${ly}px; z-index: ${zBase}; ${cardTransform}`);

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
{#if r.hidden}
	<!-- Buried behind the deck's front card + badge; not rendered. -->
{:else if zoom < 0.15}
	<!-- Heatmap mode: rendered on background canvas, nothing here -->
{:else if zoom < 0.3}
	<!-- LOD: dot only -->
	<div
		class="canvas-dot"
		class:owned={doc.is_owned}
		class:external={!doc.is_owned}
		class:peek={isPeek}
		style={posStyle}
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
		class:peek={isPeek}
		style={posStyle}
		onpointerdown={handlePointerDown}
		onpointermove={handlePointerMove}
		onpointerup={handlePointerUp}
		ondblclick={handleDblClick}
		oncontextmenu={handleContextMenu}
		onpointerenter={() => hoverCard(doc.id)}
		onpointerleave={() => hoverCard(null)}
	>
		<div class="card-title">{doc.title}</div>
		{#if showCount}<button
				class="deck-badge"
				title="Fan out {r.count} stacked documents"
				onpointerdown={(e) => e.stopPropagation()}
				onclick={(e) => { e.stopPropagation(); expandDeck(r.frontId); }}
			>{r.count}</button>{/if}
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
		class:peek={isPeek}
		style={posStyle}
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
		{#if showCount}<button
				class="deck-badge"
				title="Fan out {r.count} stacked documents"
				onpointerdown={(e) => e.stopPropagation()}
				onclick={(e) => { e.stopPropagation(); expandDeck(r.frontId); }}
			>{r.count}</button>{/if}
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
	@keyframes card-enter {
		from { opacity: 0; }
		to { opacity: 1; }
	}

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
		animation: card-enter 150ms ease-out;
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

	/* Cards peeking behind a deck's front card: dimmed but clickable — where the
	   deck is fanned open (high zoom) each peek is directly reachable; where it's
	   collapsed only their slivers show, and the front (higher z) still wins the
	   click on its body. Hovering un-dims the peek so it's clear it's live. */
	.peek {
		opacity: 0.5;
	}
	.peek:hover {
		opacity: 1;
	}

	/* Total-count badge on a deck's front card ("12" = twelve stacked here). */
	.deck-badge {
		position: absolute;
		bottom: 4px;
		right: 6px;
		font-size: 0.7rem;
		font-weight: 700;
		font-family: inherit;
		padding: 1px 7px;
		border: none;
		border-radius: 9px;
		line-height: 1.4;
		color: #fff;
		background: var(--accent);
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
		cursor: pointer;
	}
	.deck-badge:hover {
		filter: brightness(1.12);
		box-shadow: 0 2px 6px rgba(0, 0, 0, 0.45);
	}
	/* Un-skew on external (parallelogram) cards, like the reliability badge. */
	.external .deck-badge {
		transform: skewX(5deg);
	}

	.canvas-dot {
		position: absolute;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		cursor: grab;
		animation: card-enter 100ms ease-out;
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
