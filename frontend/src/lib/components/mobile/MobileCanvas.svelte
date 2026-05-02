<script lang="ts">
	/** Mobile canvas: vertical = time (Now at top), horizontal = lanes.
	 *
	 *  One lane fills the viewport at a time. Horizontal swipe pages between
	 *  lanes with snap on release (velocity-based fling, distance-based commit).
	 *  Vertical scroll within a lane pans through time (newest at top).
	 *
	 *  Phase 2.1 scope:
	 *    ✓ Vertical-time, horizontal-lane layout
	 *    ✓ Horizontal swipe to change lanes (with snap + fling)
	 *    ✓ Vertical native scroll for time pan
	 *    ✓ Tap card → open doc
	 *    ✓ iOS edge-back margin honored
	 *
	 *  Deferred (Phase 2.2+):
	 *    - Pinch-vertical zoom for time scale
	 *    - LOD adaptation (heatmap → all-lanes density strip at deep zoom-out)
	 *    - Cross-lane relationship badges
	 *    - Long-press card → action menu
	 *    - Long-press lane header → thread switcher (LaneHeader integration)
	 */
	import { canvas, mobileCanvas, setLaneIndex } from '$lib/stores/canvas.svelte';
	import { openById } from '$lib/stores/documents.svelte';

	let containerEl: HTMLElement;
	let viewportWidth = $state(390);

	type DragMode = 'none' | 'horizontal' | 'vertical';
	let dragMode = $state<DragMode>('none');
	let dragOffsetX = $state(0);
	let pointerActive = $state(false);

	let pointerStart = { x: 0, y: 0, t: 0 };
	let lastPointer = { x: 0, y: 0, t: 0 };
	let activePointerId: number | null = null;

	const EDGE_MARGIN_PX = 20; // ignore swipes that originate in iOS edge-back zone
	const LOCK_THRESHOLD_PX = 8; // min movement before locking direction
	const COMMIT_RATIO = 0.25; // distance fraction of viewport that commits a lane change
	const FLING_VELOCITY = 0.5; // px/ms for fling-based commit

	$effect(() => {
		if (typeof window === 'undefined') return;
		const sync = () => (viewportWidth = window.innerWidth);
		sync();
		window.addEventListener('resize', sync);
		return () => window.removeEventListener('resize', sync);
	});

	/** Group docs by thread id, sorted newest-first within each lane. */
	let docsByThread = $derived.by(() => {
		const map = new Map<string, typeof canvas.documents>();
		for (const t of canvas.threads) map.set(t.id, []);
		for (const d of canvas.documents) {
			const list = map.get(d.thread_id);
			if (list) list.push(d);
		}
		for (const list of map.values()) {
			list.sort(
				(a, b) =>
					new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime()
			);
		}
		return map;
	});

	let activeLaneIndex = $derived(mobileCanvas.currentLaneIndex);
	let totalLanes = $derived(canvas.threads.length);

	let lanesTransform = $derived(
		`translateX(${-activeLaneIndex * viewportWidth + dragOffsetX}px)`
	);

	function handlePointerDown(e: PointerEvent) {
		if (e.clientX < EDGE_MARGIN_PX) return; // iOS edge-back
		if (activePointerId !== null) return; // single-touch only for now
		pointerStart = { x: e.clientX, y: e.clientY, t: performance.now() };
		lastPointer = { ...pointerStart };
		dragMode = 'none';
		pointerActive = true;
		activePointerId = e.pointerId;
	}

	function handlePointerMove(e: PointerEvent) {
		if (!pointerActive || e.pointerId !== activePointerId) return;
		const dx = e.clientX - pointerStart.x;
		const dy = e.clientY - pointerStart.y;

		if (dragMode === 'none') {
			if (Math.abs(dx) > LOCK_THRESHOLD_PX && Math.abs(dx) > Math.abs(dy)) {
				dragMode = 'horizontal';
				// Capture the pointer so it keeps tracking even if it leaves the
				// container (and so native scroll can't steal it mid-swipe).
				try {
					containerEl.setPointerCapture(e.pointerId);
				} catch {
					/* ignore */
				}
			} else if (Math.abs(dy) > LOCK_THRESHOLD_PX) {
				dragMode = 'vertical';
				// Vertical = native scroll. Release the pointer; the lane scroller
				// handles it.
				pointerActive = false;
				activePointerId = null;
			}
		}

		if (dragMode === 'horizontal') {
			let offset = dx;
			// Rubber-band at edges
			if (activeLaneIndex === 0 && offset > 0) offset *= 0.3;
			if (activeLaneIndex === totalLanes - 1 && offset < 0) offset *= 0.3;
			dragOffsetX = offset;
			lastPointer = { x: e.clientX, y: e.clientY, t: performance.now() };
			e.preventDefault();
		}
	}

	function handlePointerUp(e: PointerEvent) {
		if (e.pointerId !== activePointerId && dragMode !== 'horizontal') return;
		if (containerEl?.hasPointerCapture(e.pointerId)) {
			try {
				containerEl.releasePointerCapture(e.pointerId);
			} catch {
				/* ignore */
			}
		}

		if (dragMode === 'horizontal') {
			const dt = lastPointer.t - pointerStart.t;
			const dx = lastPointer.x - pointerStart.x;
			const velocity = dt > 0 ? dx / dt : 0; // px/ms; +ve = swipe right (prev lane)

			let target = activeLaneIndex;
			if (velocity > FLING_VELOCITY) {
				target = Math.max(0, activeLaneIndex - 1);
			} else if (velocity < -FLING_VELOCITY) {
				target = Math.min(totalLanes - 1, activeLaneIndex + 1);
			} else if (dragOffsetX > viewportWidth * COMMIT_RATIO) {
				target = Math.max(0, activeLaneIndex - 1);
			} else if (dragOffsetX < -viewportWidth * COMMIT_RATIO) {
				target = Math.min(totalLanes - 1, activeLaneIndex + 1);
			}
			setLaneIndex(target);
		}

		dragOffsetX = 0;
		dragMode = 'none';
		pointerActive = false;
		activePointerId = null;
		pointerStart = { x: 0, y: 0, t: 0 };
	}

	function openDoc(id: string) {
		openById(id);
	}

	function formatRelative(iso: string): string {
		const ms = Date.now() - new Date(iso).getTime();
		const day = 86_400_000;
		if (ms < 60_000) return 'just now';
		if (ms < 3_600_000) return `${Math.round(ms / 60_000)}m ago`;
		if (ms < day) return `${Math.round(ms / 3_600_000)}h ago`;
		if (ms < day * 7) return `${Math.round(ms / day)}d ago`;
		if (ms < day * 30) return `${Math.round(ms / day / 7)}w ago`;
		return new Date(iso).toLocaleDateString();
	}
</script>

{#if !canvas.loaded}
	<div class="status" aria-live="polite">Loading…</div>
{:else if totalLanes === 0}
	<div class="status">
		<p>No lanes yet</p>
		<p class="dim">Tap + below to create one.</p>
	</div>
{:else}
	<div
		class="container"
		bind:this={containerEl}
		onpointerdown={handlePointerDown}
		onpointermove={handlePointerMove}
		onpointerup={handlePointerUp}
		onpointercancel={handlePointerUp}
		role="region"
		aria-label="Timeline lanes"
	>
		<div
			class="lanes"
			class:dragging={dragMode === 'horizontal'}
			style:transform={lanesTransform}
		>
			{#each canvas.threads as thread, i (thread.id)}
				{@const docs = docsByThread.get(thread.id) ?? []}
				<section
					class="lane"
					class:active={i === activeLaneIndex}
					style:width="{viewportWidth}px"
					aria-hidden={i !== activeLaneIndex}
				>
					<div class="lane-scroll">
						<div class="now-marker" aria-label="Now">
							<span>Now</span>
						</div>
						{#if docs.length === 0}
							<div class="lane-empty">No documents in this lane yet.</div>
						{:else}
							{#each docs as doc (doc.id)}
								<button class="card" onclick={() => openDoc(doc.id)}>
									<span
										class="provenance"
										class:owned={doc.is_owned}
										class:external={!doc.is_owned}
										aria-label={doc.is_owned ? 'Owned' : 'External'}
									></span>
									<div class="card-body">
										<div class="card-title">{doc.title || '(untitled)'}</div>
										<div class="card-time">{formatRelative(doc.modified_at)}</div>
									</div>
								</button>
							{/each}
						{/if}
					</div>
				</section>
			{/each}
		</div>
	</div>
{/if}

<style>
	.container {
		width: 100%;
		height: 100%;
		overflow: hidden;
		position: relative;
		/* allow native vertical scroll (delegated to .lane-scroll); block native
		   horizontal so our pointer handlers own swipe-between-lanes */
		touch-action: pan-y;
		background: var(--bg-primary, #1a1a20);
	}

	.lanes {
		display: flex;
		flex-direction: row;
		height: 100%;
		will-change: transform;
		transition: transform 0.22s cubic-bezier(0.2, 0.8, 0.2, 1);
	}

	.lanes.dragging {
		transition: none;
	}

	.lane {
		flex-shrink: 0;
		height: 100%;
		display: flex;
		flex-direction: column;
		border-right: 1px solid var(--border, #333);
	}

	.lane:last-child {
		border-right: none;
	}

	.lane-scroll {
		flex: 1;
		overflow-y: auto;
		-webkit-overflow-scrolling: touch;
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.now-marker {
		position: sticky;
		top: 0;
		z-index: 5;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 4px 0 8px;
		font-size: 0.65rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--accent, #f59e0b);
		background: linear-gradient(
			to bottom,
			var(--bg-primary, #1a1a20) 0%,
			var(--bg-primary, #1a1a20) 60%,
			transparent 100%
		);
	}

	.now-marker::after {
		content: '';
		flex: 1;
		height: 1px;
		background: var(--accent, #f59e0b);
		opacity: 0.6;
	}

	.card {
		display: flex;
		align-items: stretch;
		gap: 10px;
		padding: 10px 12px;
		background: var(--bg-panel, #22222a);
		border: 1px solid var(--border, #333);
		border-radius: 10px;
		text-align: left;
		cursor: pointer;
		color: var(--text-primary, #e0e0e0);
		min-height: 56px;
		font: inherit;
	}

	.card:active {
		background: var(--bg-hover, #2a2a32);
	}

	.provenance {
		flex-shrink: 0;
		width: 4px;
		border-radius: 2px;
	}

	.provenance.owned {
		background: var(--prov-owned, #10b981);
	}

	.provenance.external {
		background: var(--prov-external, #818cf8);
	}

	.card-body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.card-title {
		font-size: 0.95rem;
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.card-time {
		font-size: 0.72rem;
		color: var(--text-muted, #888);
	}

	.lane-empty {
		padding: 24px 12px;
		text-align: center;
		color: var(--text-muted, #888);
		font-size: 0.85rem;
	}

	.status {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 100%;
		gap: 6px;
		color: var(--text-muted, #888);
		font-size: 0.9rem;
	}

	.status .dim {
		opacity: 0.7;
		font-size: 0.8rem;
	}
</style>
