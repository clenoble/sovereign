<script lang="ts">
	/** Sticky top bar for the mobile shell.
	 *
	 *  Shows the active lane name and a position indicator. In Phase 1 it
	 *  reads the first thread from the canvas store as a placeholder; Phase 2
	 *  wires this to a `currentLaneIndex` field driven by horizontal swipe.
	 *
	 *  Future interactions (Phase 2):
	 *    - long-press lane name → thread switcher sheet
	 *    - pull-down on header → global search
	 */
	import { canvas, mobileCanvas, setLaneIndex } from '$lib/stores/canvas.svelte';

	let activeIndex = $derived(mobileCanvas.currentLaneIndex);
	let activeName = $derived(
		canvas.threads.length > 0 ? canvas.threads[activeIndex]?.name ?? '—' : '(no threads)'
	);
	let totalLanes = $derived(canvas.threads.length);

	function handleNameClick() {
		// Phase 2.2: open thread switcher sheet on long-press; for now a tap
		// cycles through lanes so the user can swap from the keyboard-less header.
		if (totalLanes <= 1) return;
		const next = (activeIndex + 1) % totalLanes;
		setLaneIndex(next);
	}
</script>

<header class="lane-header">
	<button class="name" onclick={handleNameClick} aria-label="Switch thread">
		<span class="name-text">{activeName}</span>
		<span class="caret" aria-hidden="true">▾</span>
	</button>

	{#if totalLanes > 1}
		<div class="dots" role="tablist" aria-label="Lane indicator">
			{#each canvas.threads as _t, i}
				<span class="dot" class:active={i === activeIndex} aria-hidden="true"></span>
			{/each}
		</div>
	{/if}
</header>

<style>
	.lane-header {
		flex-shrink: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: max(env(safe-area-inset-top), 8px) 12px 8px;
		background: var(--bg-panel, #22222a);
		border-bottom: 1px solid var(--border, #333);
		gap: 6px;
	}

	.name {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		background: none;
		border: none;
		color: var(--text-primary, #e0e0e0);
		font-size: 1rem;
		font-weight: 600;
		padding: 4px 8px;
		cursor: pointer;
		max-width: 80%;
	}

	.name-text {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.caret {
		font-size: 0.7rem;
		opacity: 0.6;
	}

	.dots {
		display: flex;
		gap: 4px;
	}

	.dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--text-muted, #666);
		opacity: 0.4;
		transition: opacity 0.15s, background 0.15s;
	}

	.dot.active {
		background: var(--accent, #f59e0b);
		opacity: 1;
	}
</style>
