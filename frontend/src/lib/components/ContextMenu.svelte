<script lang="ts">
	import { contextMenu } from '$lib/stores/app';
	import { canvas } from '$lib/stores/canvas';
	import { documents } from '$lib/stores/documents';
	import { deleteDocument, moveDocumentToThread } from '$lib/api/commands';

	let showThreadSub = $state(false);

	function handleOpen() {
		if ($contextMenu) {
			documents.openById($contextMenu.docId);
			contextMenu.set(null);
		}
	}

	async function handleDelete() {
		if ($contextMenu) {
			try {
				await deleteDocument($contextMenu.docId);
				await canvas.refresh();
			} catch (e) {
				console.error('Delete failed:', e);
			}
			contextMenu.set(null);
		}
	}

	async function handleMoveToThread(threadId: string) {
		if ($contextMenu) {
			try {
				await moveDocumentToThread($contextMenu.docId, threadId);
				await canvas.refresh();
			} catch (e) {
				console.error('Move failed:', e);
			}
			contextMenu.set(null);
		}
	}

	function handleClickOutside() {
		contextMenu.set(null);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			contextMenu.set(null);
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

{#if $contextMenu}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="ctx-backdrop" onclick={handleClickOutside}></div>
	<div
		class="ctx-menu"
		style="left: {$contextMenu.x}px; top: {$contextMenu.y}px;"
		role="menu"
	>
		<button class="ctx-item" onclick={handleOpen} role="menuitem">Open</button>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="ctx-item sub-trigger"
			onpointerenter={() => (showThreadSub = true)}
			onpointerleave={() => (showThreadSub = false)}
			role="menuitem"
		>
			Move to Thread
			{#if showThreadSub}
				<div class="sub-menu">
					{#each $canvas.threads as thread}
						{#if thread.id !== $contextMenu.threadId}
							<button class="ctx-item" onclick={() => handleMoveToThread(thread.id)}>
								{thread.name}
							</button>
						{/if}
					{/each}
				</div>
			{/if}
		</div>
		<div class="ctx-divider"></div>
		<button class="ctx-item danger" onclick={handleDelete} role="menuitem">Delete</button>
	</div>
{/if}

<style>
	.ctx-backdrop {
		position: fixed;
		inset: 0;
		z-index: 299;
	}

	.ctx-menu {
		position: fixed;
		z-index: 300;
		min-width: 160px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 4px 0;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
	}

	.ctx-item {
		display: block;
		width: 100%;
		text-align: left;
		padding: 8px 14px;
		background: none;
		border: none;
		color: var(--text-primary);
		font-size: 0.85rem;
		cursor: pointer;
		position: relative;
	}
	.ctx-item:hover {
		background: var(--bg-hover);
	}

	.ctx-item.danger {
		color: var(--error, #ef4444);
	}
	.ctx-item.danger:hover {
		background: rgba(239, 68, 68, 0.1);
	}

	.ctx-divider {
		height: 1px;
		background: var(--border);
		margin: 4px 0;
	}

	.sub-trigger {
		position: relative;
	}
	.sub-trigger::after {
		content: '\25B6';
		position: absolute;
		right: 10px;
		font-size: 0.6rem;
		color: var(--text-muted);
	}

	.sub-menu {
		position: absolute;
		left: 100%;
		top: 0;
		min-width: 140px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 4px 0;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
	}
</style>
