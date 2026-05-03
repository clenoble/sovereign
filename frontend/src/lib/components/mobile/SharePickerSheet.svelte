<script lang="ts">
	/** Sheet shown when content arrives from the OS share sheet.
	 *
	 *  The user picks a target thread; the content is saved as a new
	 *  document via the receive_shared_content Tauri command.
	 */
	import { invoke } from '@tauri-apps/api/core';
	import { app } from '$lib/stores/app.svelte';
	import { canvas } from '$lib/stores/canvas.svelte';
	import { refresh as canvasRefresh } from '$lib/stores/canvas.svelte';
	import BottomSheet from './BottomSheet.svelte';

	let saving = $state(false);
	let error = $state('');

	async function save(threadId: string) {
		if (!app.pendingShare || saving) return;
		saving = true;
		error = '';
		try {
			await invoke('receive_shared_content', {
				content: {
					content_type: app.pendingShare.contentType,
					text: app.pendingShare.text ?? null,
					url: app.pendingShare.url ?? null,
					title: app.pendingShare.title ?? null
				},
				threadId
			});
			canvasRefresh();
			app.pendingShare = null;
		} catch (e) {
			error = String(e);
		} finally {
			saving = false;
		}
	}

	function dismiss() {
		app.pendingShare = null;
	}

	function contentPreview(): string {
		if (!app.pendingShare) return '';
		const s = app.pendingShare;
		if (s.title) return s.title;
		if (s.url) return s.url;
		const text = s.text ?? '';
		return text.length > 80 ? text.slice(0, 80) + '…' : text;
	}
</script>

{#if app.pendingShare}
	<!-- Share picker is always full-detent: thread list needs the space -->
	<BottomSheet detent="full" peekHeight={0}>
		{#snippet children()}
			<div class="share-picker">
				<div class="share-header">
					<h2 class="share-title">Save to thread</h2>
					<button class="dismiss-btn" onclick={dismiss} aria-label="Dismiss">✕</button>
				</div>

				<div class="share-preview">
					<span class="preview-type">{app.pendingShare.contentType}</span>
					<span class="preview-text">{contentPreview()}</span>
				</div>

				{#if error}
					<p class="share-error">{error}</p>
				{/if}

				<ul class="thread-list" aria-label="Choose a thread">
					{#each canvas.threads as thread (thread.id)}
						<li>
							<button
								class="thread-btn"
								onclick={() => save(thread.id)}
								disabled={saving}
								aria-label="Save to {thread.name}"
							>
								<span class="thread-name">{thread.name}</span>
								{#if thread.description}
									<span class="thread-desc">{thread.description}</span>
								{/if}
							</button>
						</li>
					{:else}
						<li class="no-threads">No threads yet — create one first.</li>
					{/each}
				</ul>
			</div>
		{/snippet}
	</BottomSheet>
{/if}

<style>
	.share-picker {
		display: flex;
		flex-direction: column;
		height: 100%;
		gap: 12px;
	}

	.share-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding-bottom: 4px;
		border-bottom: 1px solid var(--border, #333);
	}

	.share-title {
		margin: 0;
		font-size: 1rem;
		font-weight: 600;
		color: var(--text-primary, #e0e0e0);
	}

	.dismiss-btn {
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 1rem;
		cursor: pointer;
		padding: 4px 8px;
	}

	.share-preview {
		display: flex;
		align-items: baseline;
		gap: 8px;
		background: var(--bg-panel, #22222a);
		border: 1px solid var(--border, #333);
		border-radius: 8px;
		padding: 10px 12px;
	}

	.preview-type {
		flex-shrink: 0;
		font-size: 0.7rem;
		font-weight: 600;
		text-transform: uppercase;
		color: var(--accent, #f59e0b);
		background: color-mix(in srgb, var(--accent, #f59e0b) 12%, transparent);
		padding: 2px 6px;
		border-radius: 4px;
	}

	.preview-text {
		font-size: 0.82rem;
		color: var(--text-secondary, #ccc);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.share-error {
		font-size: 0.82rem;
		color: var(--error, #ef4444);
		margin: 0;
	}

	.thread-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
		overflow-y: auto;
		flex: 1;
	}

	.thread-btn {
		width: 100%;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 2px;
		background: var(--bg-panel, #22222a);
		border: 1px solid var(--border, #333);
		border-radius: 8px;
		padding: 12px 14px;
		cursor: pointer;
		text-align: left;
		transition: background 0.12s;
	}
	.thread-btn:hover,
	.thread-btn:focus-visible {
		background: var(--bg-hover, #2a2a32);
		border-color: var(--accent, #f59e0b);
		outline: none;
	}
	.thread-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.thread-name {
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--text-primary, #e0e0e0);
	}

	.thread-desc {
		font-size: 0.75rem;
		color: var(--text-muted, #888);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 100%;
	}

	.no-threads {
		font-size: 0.85rem;
		color: var(--text-muted, #888);
		text-align: center;
		padding: 20px;
	}
</style>
