<script lang="ts">
	/** Centered modal listing all threads with per-lane document counts.
	 *
	 *  Opened by long-press on the LaneHeader name (or by passing
	 *  `open={true}`). Tapping a row sets the active lane and closes.
	 */
	import { canvas, mobileCanvas, setLaneIndex } from '$lib/stores/canvas.svelte';

	let { open = $bindable(false) }: { open?: boolean } = $props();

	let docCountByThread = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const d of canvas.documents) {
			counts.set(d.thread_id, (counts.get(d.thread_id) ?? 0) + 1);
		}
		return counts;
	});

	function pick(i: number) {
		setLaneIndex(i);
		open = false;
	}

	function close() {
		open = false;
	}

	function handleBackdropKey(e: KeyboardEvent) {
		if (e.key === 'Escape') close();
	}
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="lane-switcher-overlay"
		onclick={close}
		onkeydown={handleBackdropKey}
		role="dialog"
		aria-modal="true"
		aria-label="Switch lane"
	>
		<div
			class="sheet"
			onclick={(e) => e.stopPropagation()}
			onkeydown={handleBackdropKey}
			role="presentation"
		>
			<header class="sheet-header">
				<h3>Switch lane</h3>
				<button class="close" onclick={close} aria-label="Close">×</button>
			</header>

			{#if canvas.threads.length === 0}
				<div class="empty">No lanes yet — tap + to create one.</div>
			{:else}
				<ul class="lane-list">
					{#each canvas.threads as thread, i (thread.id)}
						{@const count = docCountByThread.get(thread.id) ?? 0}
						<li>
							<button
								class="lane-row"
								class:active={i === mobileCanvas.currentLaneIndex}
								onclick={() => pick(i)}
							>
								<span class="lane-name">{thread.name}</span>
								<span class="lane-count">{count} {count === 1 ? 'doc' : 'docs'}</span>
								{#if i === mobileCanvas.currentLaneIndex}
									<span class="active-mark" aria-hidden="true">●</span>
								{/if}
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	</div>
{/if}

<style>
	.lane-switcher-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.55);
		z-index: 130;
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: max(env(safe-area-inset-top), 56px);
	}

	.sheet {
		width: calc(100vw - 16px);
		max-width: 420px;
		max-height: 70vh;
		background: var(--bg-panel, #22222a);
		border: 1px solid var(--border, #333);
		border-radius: 12px;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.5);
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.sheet-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border, #333);
	}

	.sheet-header h3 {
		margin: 0;
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--text-primary, #e0e0e0);
	}

	.close {
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 1.5rem;
		line-height: 1;
		cursor: pointer;
		padding: 0 4px;
	}

	.lane-list {
		list-style: none;
		margin: 0;
		padding: 4px 0;
		overflow-y: auto;
	}

	.lane-row {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 16px;
		background: none;
		border: none;
		color: var(--text-primary, #e0e0e0);
		font: inherit;
		text-align: left;
		cursor: pointer;
		min-height: 48px;
	}

	.lane-row:active {
		background: var(--bg-hover, #2a2a32);
	}

	.lane-row.active {
		background: var(--bg-hover, #2a2a32);
	}

	.lane-name {
		flex: 1;
		font-size: 0.95rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.lane-count {
		font-size: 0.75rem;
		color: var(--text-muted, #888);
	}

	.active-mark {
		color: var(--accent, #f59e0b);
		font-size: 0.7rem;
	}

	.empty {
		padding: 24px 16px;
		text-align: center;
		color: var(--text-muted, #888);
		font-size: 0.85rem;
	}
</style>
