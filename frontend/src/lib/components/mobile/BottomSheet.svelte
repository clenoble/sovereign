<script lang="ts">
	/** Generic bottom sheet with 3 detents: peek, partial, full.
	 *
	 *  Drag the handle (or anywhere in the sheet header) up/down to move the
	 *  sheet; release snaps to the nearest detent, with a velocity threshold
	 *  for "fling" gestures.
	 *
	 *  The sheet sits above the mobile taskbar (56 px). Pass a `footer`
	 *  snippet for a pinned input row; it is hidden in peek mode.
	 */

	type Detent = 'peek' | 'partial' | 'full';

	const TASKBAR_H = 56; // keep in sync with MobileTaskbar height

	let {
		detent = $bindable<Detent>('peek'),
		peekHeight = 100,
		header,
		children,
		footer
	}: {
		detent?: Detent;
		peekHeight?: number;
		header?: import('svelte').Snippet;
		children?: import('svelte').Snippet;
		footer?: import('svelte').Snippet;
	} = $props();

	let viewportHeight = $state(800);
	let dragOffset = $state(0); // px relative to current detent (positive = dragging down = shrinking)
	let isDragging = $state(false);

	let pointerStart = { y: 0, t: 0 };
	let lastPointer = { y: 0, t: 0 };

	$effect(() => {
		if (typeof window === 'undefined') return;
		const sync = () => (viewportHeight = window.innerHeight);
		sync();
		window.addEventListener('resize', sync);
		return () => window.removeEventListener('resize', sync);
	});

	function detentHeight(d: Detent): number {
		const usable = viewportHeight - TASKBAR_H;
		if (d === 'peek') return peekHeight;
		if (d === 'partial') return Math.round(usable * 0.5);
		return Math.round(usable * 0.88);
	}

	let currentHeight = $derived(detentHeight(detent) - dragOffset);
	let clampedHeight = $derived(
		Math.min(detentHeight('full'), Math.max(peekHeight * 0.5, currentHeight))
	);

	function handlePointerDown(e: PointerEvent) {
		isDragging = true;
		pointerStart = { y: e.clientY, t: performance.now() };
		lastPointer = { ...pointerStart };
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
	}

	function handlePointerMove(e: PointerEvent) {
		if (!isDragging) return;
		const dy = e.clientY - pointerStart.y;
		dragOffset = dy;
		lastPointer = { y: e.clientY, t: performance.now() };
	}

	function handlePointerUp(e: PointerEvent) {
		if (!isDragging) return;
		isDragging = false;
		(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);

		const dt = lastPointer.t - pointerStart.t;
		const dy = lastPointer.y - pointerStart.y;
		const velocity = dt > 0 ? dy / dt : 0; // px/ms; positive = moved down

		if (velocity > 0.6) {
			detent = detent === 'full' ? 'partial' : 'peek';
		} else if (velocity < -0.6) {
			detent = detent === 'peek' ? 'partial' : 'full';
		} else {
			const target = clampedHeight;
			const candidates: Detent[] = ['peek', 'partial', 'full'];
			let best: Detent = detent;
			let bestDist = Infinity;
			for (const c of candidates) {
				const d = Math.abs(detentHeight(c) - target);
				if (d < bestDist) {
					bestDist = d;
					best = c;
				}
			}
			detent = best;
		}
		dragOffset = 0;
	}

	function cycleDetent() {
		if (detent === 'peek') detent = 'partial';
		else if (detent === 'partial') detent = 'full';
		else detent = 'peek';
	}
</script>

<aside
	class="sheet"
	class:dragging={isDragging}
	style:height="{clampedHeight}px"
	aria-label="Bottom sheet"
>
	<div
		class="grip"
		onpointerdown={handlePointerDown}
		onpointermove={handlePointerMove}
		onpointerup={handlePointerUp}
		onpointercancel={handlePointerUp}
		ondblclick={cycleDetent}
		role="button"
		tabindex="0"
		aria-label="Drag handle (double-click to cycle)"
	>
		<span class="handle-bar"></span>
		{#if header}
			<div class="sheet-header">{@render header()}</div>
		{/if}
	</div>

	<div class="content">
		{#if children}
			{@render children()}
		{/if}
	</div>

	{#if footer && detent !== 'peek'}
		<div class="footer-slot">
			{@render footer()}
		</div>
	{/if}
</aside>

<style>
	.sheet {
		position: fixed;
		left: 0;
		right: 0;
		/* Sit above the taskbar, not overlapping it */
		bottom: calc(env(safe-area-inset-bottom, 0px) + 56px);
		background: var(--bg-panel, #22222a);
		border-top: 1px solid var(--border, #333);
		border-radius: 16px 16px 0 0;
		box-shadow: 0 -8px 24px rgba(0, 0, 0, 0.4);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		transition: height 0.18s ease;
		z-index: 90;
	}

	.sheet.dragging {
		transition: none;
	}

	.grip {
		flex-shrink: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		padding: 8px 12px 4px;
		cursor: grab;
		touch-action: none;
		user-select: none;
	}

	.grip:active {
		cursor: grabbing;
	}

	.handle-bar {
		width: 36px;
		height: 4px;
		border-radius: 2px;
		background: var(--text-muted, #666);
		opacity: 0.5;
	}

	.sheet-header {
		width: 100%;
	}

	.content {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		padding: 6px 12px 8px;
	}

	.footer-slot {
		flex-shrink: 0;
		padding: 8px 12px max(env(safe-area-inset-bottom, 0px), 8px);
		border-top: 1px solid var(--border, #333);
	}
</style>
