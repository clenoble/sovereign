<script lang="ts">
	/** Sticky top bar for the mobile shell.
	 *
	 *  Tap on the lane name → opens the LaneSwitcherSheet (instant).
	 *  Long-press → also opens the switcher (mobile convention; same target).
	 *  Lane-position dots show progress through the thread list.
	 *
	 *  Future (Phase 3): pull-down on this header → global search.
	 */
	import { canvas, mobileCanvas } from '$lib/stores/canvas.svelte';
	import { longPress } from '$lib/actions/longPress';
	import LaneSwitcherSheet from './LaneSwitcherSheet.svelte';

	let activeIndex = $derived(mobileCanvas.currentLaneIndex);
	let activeName = $derived(
		canvas.threads.length > 0 ? canvas.threads[activeIndex]?.name ?? '—' : '(no threads)'
	);
	let totalLanes = $derived(canvas.threads.length);

	let switcherOpen = $state(false);

	function openSwitcher() {
		if (totalLanes === 0) return;
		switcherOpen = true;
	}
</script>

<header class="lane-header">
	<button
		class="name"
		onclick={openSwitcher}
		use:longPress={{ onLongPress: openSwitcher }}
		aria-label="Switch lane"
		aria-haspopup="dialog"
	>
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

<LaneSwitcherSheet bind:open={switcherOpen} />

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
