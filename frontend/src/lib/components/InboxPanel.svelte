<script lang="ts">
	import { onMount } from 'svelte';
	import { app } from '$lib/stores/app.svelte';
	import { contactsState, loadContacts } from '$lib/stores/contacts.svelte';

	// Drag state
	let position = $state({ x: 120, y: 200 });
	let dragging = false;
	let dragStart = { x: 0, y: 0 };
	let dragOriginal = { x: 0, y: 0 };

	onMount(() => {
		loadContacts();
	});

	function openContact(contactId: string) {
		app.contactPanelState = { contactId };
	}

	// Drag handlers
	function handleHeaderPointerDown(e: PointerEvent) {
		if (e.button !== 0) return;
		dragging = true;
		dragStart = { x: e.clientX, y: e.clientY };
		dragOriginal = { x: position.x, y: position.y };
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
	}

	function handleHeaderPointerMove(e: PointerEvent) {
		if (!dragging) return;
		position = {
			x: dragOriginal.x + (e.clientX - dragStart.x),
			y: dragOriginal.y + (e.clientY - dragStart.y)
		};
	}

	function handleHeaderPointerUp(e: PointerEvent) {
		dragging = false;
		(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
	}
</script>

{#if app.inboxVisible}
	<div class="inbox-panel" style="left: {position.x}px; top: {position.y}px;">
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="inbox-header"
			onpointerdown={handleHeaderPointerDown}
			onpointermove={handleHeaderPointerMove}
			onpointerup={handleHeaderPointerUp}
		>
			<h3>Contacts ({contactsState.contacts.length})</h3>
			<button class="close-btn" onclick={() => app.inboxVisible = false}>&times;</button>
		</div>

		<div class="contact-list">
			{#each contactsState.contacts as contact (contact.id)}
				<button class="contact-row" onclick={() => openContact(contact.id)}>
					<div class="contact-avatar">
						{contact.name.charAt(0).toUpperCase()}
					</div>
					<div class="contact-info">
						<div class="contact-name">{contact.name}</div>
						<div class="contact-channels">
							{contact.channels.join(' · ')}
						</div>
					</div>
					{#if contact.unread_count > 0}
						<span class="unread-badge">{contact.unread_count}</span>
					{/if}
				</button>
			{/each}

			{#if contactsState.contacts.length === 0}
				<div class="empty">No contacts yet</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.inbox-panel {
		position: fixed;
		width: 300px;
		max-height: 500px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 12px;
		z-index: 90;
		display: flex;
		flex-direction: column;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
	}

	.inbox-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
		cursor: grab;
		user-select: none;
	}

	.inbox-header:active {
		cursor: grabbing;
	}

	.inbox-header h3 {
		margin: 0;
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		font-size: 1.2rem;
		padding: 0 4px;
	}

	.close-btn:hover {
		color: var(--text-primary);
	}

	.contact-list {
		flex: 1;
		overflow-y: auto;
	}

	.contact-row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		padding: 10px 14px;
		background: none;
		border: none;
		border-bottom: 1px solid var(--border);
		cursor: pointer;
		text-align: left;
		color: var(--text-primary);
	}

	.contact-row:hover {
		background: var(--bg-hover);
	}

	.contact-avatar {
		width: 32px;
		height: 32px;
		border-radius: 50%;
		background: var(--accent);
		color: #000;
		display: flex;
		align-items: center;
		justify-content: center;
		font-weight: 600;
		font-size: 0.85rem;
		flex-shrink: 0;
	}

	.contact-info {
		flex: 1;
		min-width: 0;
	}

	.contact-name {
		font-size: 0.85rem;
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.contact-channels {
		font-size: 0.7rem;
		color: var(--text-muted);
		margin-top: 2px;
	}

	.unread-badge {
		background: var(--error, #ef4444);
		color: #fff;
		font-size: 0.7rem;
		font-weight: 600;
		padding: 2px 6px;
		border-radius: 10px;
		flex-shrink: 0;
	}

	.empty {
		padding: 24px 16px;
		color: var(--text-muted);
		font-size: 0.85rem;
		text-align: center;
	}
</style>
