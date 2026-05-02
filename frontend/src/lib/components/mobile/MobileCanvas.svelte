<script lang="ts">
	/** Mobile canvas: vertical = time (Now at top), horizontal = lanes.
	 *
	 *  One lane fills the viewport. Horizontal swipe pages between lanes with
	 *  snap on release. Cards inside a lane are absolutely positioned by their
	 *  modified_at timestamp scaled by mobileCanvas.pxPerMs (with anti-collision
	 *  push-down so same-time cards stack instead of overlapping). Two-finger
	 *  vertical pinch adjusts pxPerMs (= time interval), anchored at the
	 *  midpoint so the time under your fingers stays put.
	 *
	 *  Phase 2.3 scope (this file):
	 *    ✓ Cards positioned by time (Y = (now - modified_at) * pxPerMs)
	 *    ✓ Anti-collision: cards never overlap (cascade push-down)
	 *    ✓ Two-finger vertical pinch → adjust pxPerMs with midpoint anchor
	 *    ✓ Lane swipe + native scroll + long-press menu + cross-lane badges
	 *      from 2.1/2.2 still work
	 *
	 *  Deferred (Phase 2.4):
	 *    - LOD adaptation: at very low pxPerMs, pivot to all-lanes density strip
	 *    - Periodic Now-tick re-layout (cards drift as time passes)
	 */
	import {
		canvas,
		mobileCanvas,
		setLaneIndex,
		setMobilePxPerMs,
		LOD_THRESHOLD_PX_PER_MS,
		MOBILE_PX_PER_DAY
	} from '$lib/stores/canvas.svelte';
	import { openById } from '$lib/stores/documents.svelte';
	import { app } from '$lib/stores/app.svelte';
	import { longPress } from '$lib/actions/longPress';

	const MS_PER_DAY = 86_400_000;
	const NOW_TICK_INTERVAL_MS = 15 * 60_000; // 15 minutes

	let containerEl = $state<HTMLElement>();
	let viewportWidth = $state(390);

	type DragMode = 'none' | 'horizontal' | 'vertical' | 'pinch';
	let dragMode = $state<DragMode>('none');
	let dragOffsetX = $state(0);

	type PointerInfo = { x: number; y: number };
	const pointers = new Map<number, PointerInfo>();
	let primaryPointerId: number | null = null;
	let pointerStart = { x: 0, y: 0, t: 0 };
	let lastPointer = { x: 0, y: 0, t: 0 };

	interface PinchState {
		initialDist: number; // |p1.y - p2.y| at start
		initialPxPerMs: number;
		midpointY: number; // (p1.y + p2.y) / 2 at start (screen coord)
		anchorTimeMs: number; // time at midpoint (ms before "now")
		laneScrollEl: HTMLElement;
		containerTop: number; // bounding-rect top of laneScrollEl at start
	}
	let pinch: PinchState | null = null;

	const EDGE_MARGIN_PX = 20; // ignore swipes that originate in iOS edge-back zone
	const LOCK_THRESHOLD_PX = 8; // min movement before locking direction
	const COMMIT_RATIO = 0.25; // distance fraction of viewport that commits a lane change
	const FLING_VELOCITY = 0.5; // px/ms for fling-based commit
	const PINCH_VERTICAL_THRESHOLD_PX = 30; // |p1.y - p2.y| above this is a vertical pinch
	const CARD_HEIGHT_PX = 56; // matches .card min-height in CSS
	const CARD_GAP_PX = 8;
	const TRACK_TOP_PADDING = 36; // space below the Now marker
	const TRACK_BOTTOM_PADDING = 80; // breathing room at the oldest end

	$effect(() => {
		if (typeof window === 'undefined') return;
		const sync = () => (viewportWidth = window.innerWidth);
		sync();
		window.addEventListener('resize', sync);
		return () => window.removeEventListener('resize', sync);
	});

	/** Periodic tick to force time-dependent layouts to re-derive. Cards
	 *  drift down as time passes and the density-bucket grid re-aligns; a
	 *  15-minute interval is plenty of resolution and avoids thrashing. */
	let nowTick = $state(0);
	$effect(() => {
		if (typeof window === 'undefined') return;
		const id = window.setInterval(() => {
			nowTick++;
		}, NOW_TICK_INTERVAL_MS);
		return () => clearInterval(id);
	});

	let isLowDetail = $derived(mobileCanvas.pxPerMs < LOD_THRESHOLD_PX_PER_MS);

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

	/** docId → number of relationships connecting to a doc in a *different*
	 *  thread. Drives the ↔N badge on cards. */
	let crossLaneCount = $derived.by(() => {
		const docToThread = new Map<string, string>();
		for (const d of canvas.documents) docToThread.set(d.id, d.thread_id);
		const counts = new Map<string, number>();
		for (const r of canvas.relationships) {
			const fromThread = docToThread.get(r.from_doc_id);
			const toThread = docToThread.get(r.to_doc_id);
			if (!fromThread || !toThread) continue;
			if (fromThread === toThread) continue;
			counts.set(r.from_doc_id, (counts.get(r.from_doc_id) ?? 0) + 1);
			counts.set(r.to_doc_id, (counts.get(r.to_doc_id) ?? 0) + 1);
		}
		return counts;
	});

	interface PlacedCard {
		doc: (typeof canvas.documents)[number];
		y: number;
		xLane: number;
	}

	interface PlacedLane {
		thread: (typeof canvas.threads)[number];
		cards: PlacedCard[];
		trackHeight: number;
	}

	/** Lay out each lane: cards positioned absolutely by time, with cascade
	 *  push-down to prevent overlap. Re-runs whenever docs, threads,
	 *  relationships, pxPerMs, or the 15-min nowTick changes. */
	let placedLanes = $derived.by<PlacedLane[]>(() => {
		void nowTick; // dependency: drift cards as time passes
		const now = Date.now();
		const px = mobileCanvas.pxPerMs;
		return canvas.threads.map((thread) => {
			const docs = docsByThread.get(thread.id) ?? [];
			let lastBottom = TRACK_TOP_PADDING;
			const cards: PlacedCard[] = docs.map((doc) => {
				const target =
					(now - new Date(doc.modified_at).getTime()) * px + TRACK_TOP_PADDING;
				const y = Math.max(target, lastBottom);
				lastBottom = y + CARD_HEIGHT_PX + CARD_GAP_PX;
				return { doc, y, xLane: crossLaneCount.get(doc.id) ?? 0 };
			});
			return {
				thread,
				cards,
				trackHeight: lastBottom + TRACK_BOTTOM_PADDING
			};
		});
	});

	interface DensityLane {
		thread: (typeof canvas.threads)[number];
		counts: number[]; // doc count per day-bucket, newest first
		total: number;
		maxCount: number;
	}

	const DENSITY_BUCKET_MS = MS_PER_DAY;
	const DENSITY_MAX_BUCKETS = 365; // cap a year of data so cells stay legible

	/** Per-lane day-bucket activity counts for the deep-zoom density strip. */
	let densityLanes = $derived.by<DensityLane[]>(() => {
		void nowTick;
		const now = Date.now();
		// Find the oldest doc across all threads to size the time range.
		let oldestMs = now;
		for (const d of canvas.documents) {
			const t = new Date(d.modified_at).getTime();
			if (t < oldestMs) oldestMs = t;
		}
		const span = now - oldestMs;
		const buckets = Math.min(
			DENSITY_MAX_BUCKETS,
			Math.max(1, Math.ceil(span / DENSITY_BUCKET_MS) + 1)
		);
		return canvas.threads.map((thread) => {
			const docs = docsByThread.get(thread.id) ?? [];
			const counts = new Array(buckets).fill(0) as number[];
			let max = 0;
			for (const d of docs) {
				const t = new Date(d.modified_at).getTime();
				const i = Math.floor((now - t) / DENSITY_BUCKET_MS);
				if (i >= 0 && i < buckets) {
					counts[i]++;
					if (counts[i] > max) max = counts[i];
				}
			}
			return { thread, counts, total: docs.length, maxCount: max };
		});
	});

	let activeLaneIndex = $derived(mobileCanvas.currentLaneIndex);
	let totalLanes = $derived(canvas.threads.length);

	let lanesTransform = $derived(
		`translateX(${-activeLaneIndex * viewportWidth + dragOffsetX}px)`
	);

	function activeLaneScrollEl(): HTMLElement | null {
		if (!containerEl) return null;
		const lanes = containerEl.querySelectorAll('.lane');
		const el = lanes[activeLaneIndex] as HTMLElement | undefined;
		return (el?.querySelector('.lane-scroll') as HTMLElement | null) ?? null;
	}

	function startPinch() {
		const ids = [...pointers.keys()];
		if (ids.length < 2) return;
		const p1 = pointers.get(ids[0])!;
		const p2 = pointers.get(ids[1])!;
		const dy = Math.abs(p1.y - p2.y);
		if (dy < PINCH_VERTICAL_THRESHOLD_PX) return; // fingers side-by-side, not a vertical pinch

		const laneScrollEl = isLowDetail ? null : activeLaneScrollEl();
		const midY = (p1.y + p2.y) / 2;

		let anchorTimeMs = 0;
		let containerTop = 0;
		if (laneScrollEl) {
			containerTop = laneScrollEl.getBoundingClientRect().top;
			const midRelToScroll = midY - containerTop;
			const anchorPx =
				laneScrollEl.scrollTop + midRelToScroll - TRACK_TOP_PADDING;
			anchorTimeMs = anchorPx / mobileCanvas.pxPerMs;
		}

		pinch = {
			initialDist: dy,
			initialPxPerMs: mobileCanvas.pxPerMs,
			midpointY: midY,
			anchorTimeMs,
			laneScrollEl: laneScrollEl as HTMLElement,
			containerTop
		};
		dragMode = 'pinch';
		dragOffsetX = 0;
	}

	function updatePinch() {
		if (!pinch) return;
		const ids = [...pointers.keys()];
		if (ids.length < 2) return;
		const p1 = pointers.get(ids[0])!;
		const p2 = pointers.get(ids[1])!;
		const currentDist = Math.abs(p1.y - p2.y);
		if (currentDist < 1) return;

		const scale = currentDist / pinch.initialDist;
		setMobilePxPerMs(pinch.initialPxPerMs * scale);

		// In density (LOD) mode there's no per-lane scroller to anchor; the
		// pxPerMs change alone drives the LOD switch when the threshold crosses.
		if (!pinch.laneScrollEl) return;

		// Re-anchor: place anchorTimeMs at the original midpoint Y.
		const anchorPx = pinch.anchorTimeMs * mobileCanvas.pxPerMs;
		const midRelToScroll = pinch.midpointY - pinch.containerTop;
		pinch.laneScrollEl.scrollTop =
			anchorPx + TRACK_TOP_PADDING - midRelToScroll;
	}

	function endPinch() {
		pinch = null;
		dragMode = 'none';
	}

	function handlePointerDown(e: PointerEvent) {
		// Track every pointer for multi-touch pinch detection.
		pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });

		// If we now have ≥2 pointers, try to start a pinch.
		if (pointers.size >= 2) {
			startPinch();
			return;
		}

		// First (single) pointer: start swipe-detection state machine, but
		// honor the iOS edge-back margin.
		if (e.clientX < EDGE_MARGIN_PX) {
			pointers.delete(e.pointerId); // disengage so we don't track it
			return;
		}
		primaryPointerId = e.pointerId;
		pointerStart = { x: e.clientX, y: e.clientY, t: performance.now() };
		lastPointer = { ...pointerStart };
		dragMode = 'none';
	}

	function handlePointerMove(e: PointerEvent) {
		if (!pointers.has(e.pointerId)) return;
		pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });

		// Active pinch: update zoom regardless of direction-lock.
		if (dragMode === 'pinch') {
			updatePinch();
			e.preventDefault();
			return;
		}

		// If a second pointer just arrived, attempt pinch.
		if (pointers.size >= 2) {
			startPinch();
			return;
		}

		// Single-pointer swipe path — only the primary pointer drives lane swipe.
		if (e.pointerId !== primaryPointerId) return;
		const dx = e.clientX - pointerStart.x;
		const dy = e.clientY - pointerStart.y;

		if (dragMode === 'none') {
			if (Math.abs(dx) > LOCK_THRESHOLD_PX && Math.abs(dx) > Math.abs(dy)) {
				dragMode = 'horizontal';
				try {
					containerEl?.setPointerCapture(e.pointerId);
				} catch {
					/* ignore */
				}
			} else if (Math.abs(dy) > LOCK_THRESHOLD_PX) {
				dragMode = 'vertical';
				// Native vertical scroll handles it from here; stop tracking the
				// primary pointer so subsequent moves don't re-engage.
				primaryPointerId = null;
			}
		}

		if (dragMode === 'horizontal') {
			let offset = dx;
			if (activeLaneIndex === 0 && offset > 0) offset *= 0.3;
			if (activeLaneIndex === totalLanes - 1 && offset < 0) offset *= 0.3;
			dragOffsetX = offset;
			lastPointer = { x: e.clientX, y: e.clientY, t: performance.now() };
			e.preventDefault();
		}
	}

	function handlePointerUp(e: PointerEvent) {
		const wasTracked = pointers.has(e.pointerId);
		pointers.delete(e.pointerId);

		if (containerEl?.hasPointerCapture(e.pointerId)) {
			try {
				containerEl.releasePointerCapture(e.pointerId);
			} catch {
				/* ignore */
			}
		}

		// End pinch when we drop below 2 pointers.
		if (dragMode === 'pinch') {
			if (pointers.size < 2) endPinch();
			return;
		}

		if (!wasTracked) return;

		if (dragMode === 'horizontal' && e.pointerId === primaryPointerId) {
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

		if (e.pointerId === primaryPointerId) {
			primaryPointerId = null;
			pointerStart = { x: 0, y: 0, t: 0 };
		}
		dragOffsetX = 0;
		dragMode = 'none';
	}

	function openDoc(id: string) {
		openById(id);
	}

	/** Tap on a density-strip column → drill into that lane and reset the
	 *  zoom to the default per-day pixel scale so the user lands in the
	 *  normal per-lane card view. */
	function pickLaneFromDensity(i: number) {
		setLaneIndex(i);
		setMobilePxPerMs(MOBILE_PX_PER_DAY / MS_PER_DAY);
	}

	/** Map a doc count to a heatmap-cell opacity. */
	function densityOpacity(count: number, max: number): number {
		if (count <= 0) return 0;
		if (max <= 1) return 0.65;
		// Logarithmic-ish ramp so a single doc still registers visibly.
		return Math.min(1, 0.25 + 0.75 * (Math.log(count + 1) / Math.log(max + 1)));
	}

	/** Zoom button handler (mouse + accessibility fallback for pinch).
	 *  factor > 1 zooms in (more pixels per ms), factor < 1 zooms out.
	 *  In per-lane mode anchors at the vertical center of the visible
	 *  viewport so the user's view doesn't jump. In density mode (LOD)
	 *  there's no scroll anchor; just adjust pxPerMs. */
	function zoomBy(factor: number) {
		const laneScrollEl = isLowDetail ? null : activeLaneScrollEl();
		if (!laneScrollEl) {
			setMobilePxPerMs(mobileCanvas.pxPerMs * factor);
			return;
		}
		const rect = laneScrollEl.getBoundingClientRect();
		const visibleH = rect.height;
		const anchorScreenY = visibleH / 2; // center of visible scroller
		const oldPx = mobileCanvas.pxPerMs;
		const anchorTimeMs =
			(laneScrollEl.scrollTop + anchorScreenY - TRACK_TOP_PADDING) / oldPx;
		setMobilePxPerMs(oldPx * factor);
		const newPx = mobileCanvas.pxPerMs;
		laneScrollEl.scrollTop =
			anchorTimeMs * newPx + TRACK_TOP_PADDING - anchorScreenY;
	}

	function showCardMenu(
		e: PointerEvent | MouseEvent,
		doc: { id: string; thread_id: string }
	) {
		const menuW = 200;
		const menuH = 220;
		const x = Math.min(e.clientX, window.innerWidth - menuW - 8);
		const y = Math.min(e.clientY, window.innerHeight - menuH - 8);
		app.contextMenu = {
			x: Math.max(8, x),
			y: Math.max(8, y),
			docId: doc.id,
			threadId: doc.thread_id
		};
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
		<!-- Zoom controls — overlay in upper-right of viewport. Mouse + a11y
		     fallback for two-finger pinch. Hidden in density mode where the
		     buttons would suggest the same per-lane scroll model. -->
		{#if !isLowDetail}
			<div class="zoom-controls" aria-label="Zoom time scale">
				<button class="zoom-btn" onclick={() => zoomBy(1.5)} aria-label="Zoom in">+</button>
				<button class="zoom-btn" onclick={() => zoomBy(1 / 1.5)} aria-label="Zoom out">−</button>
			</div>
		{/if}

		{#if isLowDetail}
			<!-- All-lanes density strip (deep zoom-out LOD). One column per
			     thread, day-bucket activity heatmap. Tap a column to drill
			     in (resets pxPerMs to default and switches to that lane). -->
			<div class="density-strip" role="list" aria-label="All lanes overview">
				{#each densityLanes as lane, i (lane.thread.id)}
					<button
						class="density-column"
						class:active={i === activeLaneIndex}
						onclick={() => pickLaneFromDensity(i)}
						style:flex="1 1 0"
						aria-label="{lane.thread.name}, {lane.total} document{lane.total === 1
							? ''
							: 's'}"
					>
						<div class="density-name">{lane.thread.name}</div>
						<div class="density-count">{lane.total}</div>
						<div class="density-cells">
							{#each lane.counts as count, di}
								<div
									class="density-cell"
									class:empty={count === 0}
									style:opacity={densityOpacity(count, lane.maxCount)}
									title={count > 0
										? `${count} doc${count === 1 ? '' : 's'} · ${di}d ago`
										: ''}
								></div>
							{/each}
						</div>
					</button>
				{/each}
			</div>
		{:else}
		<div
			class="lanes"
			class:dragging={dragMode === 'horizontal'}
			style:transform={lanesTransform}
		>
			{#each placedLanes as lane, i (lane.thread.id)}
				<section
					class="lane"
					class:active={i === activeLaneIndex}
					style:width="{viewportWidth}px"
					aria-hidden={i !== activeLaneIndex}
				>
					<div class="lane-scroll">
						<div class="lane-track" style:height="{lane.trackHeight}px">
							<div class="now-marker" aria-label="Now">
								<span>Now</span>
							</div>
							{#if lane.cards.length === 0}
								<div class="lane-empty">No documents in this lane yet.</div>
							{:else}
								{#each lane.cards as { doc, y, xLane } (doc.id)}
									<button
										class="card"
										style:top="{y}px"
										onclick={() => openDoc(doc.id)}
										use:longPress={{ onLongPress: (e) => showCardMenu(e, doc) }}
										oncontextmenu={(e) => {
											e.preventDefault();
											showCardMenu(e, doc);
										}}
									>
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
										{#if xLane > 0}
											<span
												class="cross-lane-badge"
												title="{xLane} link{xLane === 1 ? '' : 's'} to other lanes"
												aria-label="{xLane} cross-lane link{xLane === 1 ? '' : 's'}"
											>
												↔{xLane}
											</span>
										{/if}
									</button>
								{/each}
							{/if}
						</div>
					</div>
				</section>
			{/each}
		</div>
		{/if}
	</div>
{/if}

<style>
	.container {
		width: 100%;
		height: 100%;
		overflow: hidden;
		position: relative;
		/* Allow native vertical scroll inside .lane-scroll; block native
		   horizontal so our pointer handlers own swipe-between-lanes.
		   pinch-zoom is handled manually so disable browser pinch. */
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
		position: relative;
	}

	.lane-track {
		position: relative;
		width: 100%;
		padding: 0 12px;
		box-sizing: border-box;
	}

	.now-marker {
		position: sticky;
		top: 0;
		z-index: 5;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 0 8px;
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
		position: absolute;
		left: 12px;
		right: 12px;
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
		height: 56px;
		box-sizing: border-box;
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
		justify-content: center;
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

	.cross-lane-badge {
		flex-shrink: 0;
		align-self: center;
		font-size: 0.7rem;
		font-weight: 600;
		padding: 2px 6px;
		border-radius: 8px;
		background: var(--bg-hover, #2a2a32);
		color: var(--accent, #f59e0b);
		border: 1px solid var(--accent, #f59e0b);
		line-height: 1;
		letter-spacing: 0.02em;
	}

	.lane-empty {
		position: absolute;
		left: 0;
		right: 0;
		top: 60px;
		text-align: center;
		color: var(--text-muted, #888);
		font-size: 0.85rem;
		padding: 24px 12px;
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

	.zoom-controls {
		position: absolute;
		top: 8px;
		right: 8px;
		z-index: 20;
		display: flex;
		flex-direction: column;
		gap: 4px;
		opacity: 0.85;
	}

	.zoom-btn {
		width: 32px;
		height: 32px;
		border-radius: 50%;
		background: var(--bg-panel, #22222a);
		color: var(--text-primary, #e0e0e0);
		border: 1px solid var(--border, #333);
		font-size: 1rem;
		font-weight: 600;
		line-height: 1;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
	}

	.zoom-btn:active {
		background: var(--bg-hover, #2a2a32);
	}

	/* ----- Density strip (deep zoom-out LOD) ----- */

	.density-strip {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: row;
		padding: 8px 4px;
		gap: 2px;
		background: var(--bg-primary, #1a1a20);
	}

	.density-column {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		min-width: 0;
		padding: 6px 4px 4px;
		background: var(--bg-panel, #22222a);
		border: 1px solid var(--border, #333);
		border-radius: 8px;
		cursor: pointer;
		color: var(--text-primary, #e0e0e0);
		font: inherit;
		text-align: center;
		overflow: hidden;
	}

	.density-column.active {
		border-color: var(--accent, #f59e0b);
	}

	.density-column:active {
		background: var(--bg-hover, #2a2a32);
	}

	.density-name {
		font-size: 0.7rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		margin-bottom: 2px;
	}

	.density-count {
		font-size: 0.65rem;
		color: var(--text-muted, #888);
		margin-bottom: 4px;
	}

	.density-cells {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 1px;
		overflow: hidden;
		min-height: 0;
	}

	.density-cell {
		flex: 1 1 0;
		min-height: 1px;
		background: var(--accent, #f59e0b);
		border-radius: 1px;
	}

	.density-cell.empty {
		background: transparent;
	}
</style>
