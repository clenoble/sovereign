<script lang="ts">
	import { onMount } from 'svelte';
	import { app } from '$lib/stores/app.svelte';
	import { contactsState, loadContacts } from '$lib/stores/contacts.svelte';

	onMount(() => {
		loadContacts();
	});

	function openContact(contactId: string) {
		app.contactPanelState = { contactId };
	}
</script>

{#if app.inboxVisible}
	<div class="inbox-panel">
		<div class="inbox-header">
			<h3>Inbox</h3>
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
		top: 0;
		left: 0;
		width: 340px;
		height: calc(100vh - 44px);
		background: var(--bg-panel);
		border-right: 1px solid var(--border);
		z-index: 60;
		display: flex;
		flex-direction: column;
	}

	.inbox-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
	}

	.inbox-header h3 {
		margin: 0;
		font-size: 1rem;
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
		padding: 10px 16px;
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
		width: 36px;
		height: 36px;
		border-radius: 50%;
		background: var(--accent);
		color: #fff;
		display: flex;
		align-items: center;
		justify-content: center;
		font-weight: 600;
		font-size: 0.9rem;
		flex-shrink: 0;
	}

	.contact-info {
		flex: 1;
		min-width: 0;
	}

	.contact-name {
		font-size: 0.9rem;
		font-weight: 500;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.contact-channels {
		font-size: 0.75rem;
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
