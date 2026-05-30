<script lang="ts">
	import { app } from '$lib/stores/app.svelte';
	import { canvas, refresh as canvasRefresh } from '$lib/stores/canvas.svelte';
	import { openById } from '$lib/stores/documents.svelte';
	import { deleteDocument, moveDocumentToThread } from '$lib/api/commands';

	let showThreadSub = $state(false);

	function handleOpen() {
		if (app.contextMenu) {
			openById(app.contextMenu.docId);
			app.contextMenu = null;
		}
	}

	async function handleDelete() {
		if (app.contextMenu) {
			try {
				await deleteDocument(app.contextMenu.docId);
				await canvasRefresh();
			} catch (e) {
				console.error('Delete failed:', e);
			}
			app.contextMenu = null;
		}
	}

	async function handleMoveToThread(threadId: string) {
		if (app.contextMenu) {
			try {
				await moveDocumentToThread(app.contextMenu.docId, threadId);
				await canvasRefresh();
			} catch (e) {
				console.error('Move failed:', e);
			}
			app.contextMenu = null;
		}
	}

	function handleClickOutside() {
		app.contextMenu = null;
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			app.contextMenu = null;
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

{#if app.contextMenu}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="ctx-backdrop" onclick={handleClickOutside}></div>
	<div
		class="ctx-menu"
		style="left: {app.contextMenu.x}px; top: {app.contextMenu.y}px;"
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
					{#each canvas.threads as thread}
						{#if thread.id !== app.contextMenu.threadId}
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
