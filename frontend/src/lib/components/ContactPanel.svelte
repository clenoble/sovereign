<script lang="ts">
	import { onMount } from 'svelte';
	import { app } from '$lib/stores/app.svelte';
	import {
		getContactDetail,
		listMessages,
		markMessageRead,
		type ContactDetailDto,
		type MessageDto
	} from '$lib/api/commands';

	let contact = $state<ContactDetailDto | null>(null);
	let selectedConvIdx = $state(0);
	let messages = $state<MessageDto[]>([]);
	let loadingMessages = $state(false);

	// Drag state
	let position = $state({ x: 120, y: 60 });
	let dragging = false;
	let dragStart = { x: 0, y: 0 };
	let dragOriginal = { x: 0, y: 0 };

	$effect(() => {
		const state = app.contactPanelState;
		if (state) {
			loadContact(state.contactId, state.conversationId);
		} else {
			contact = null;
			messages = [];
		}
	});

	async function loadContact(id: string, conversationId?: string) {
		try {
			contact = await getContactDetail(id);
			// If a specific conversation was requested, select it
			let idx = 0;
			if (conversationId && contact.conversations.length > 0) {
				const found = contact.conversations.findIndex(c => c.id === conversationId);
				if (found >= 0) idx = found;
			}
			selectedConvIdx = idx;
			if (contact.conversations.length > 0) {
				await loadConversationMessages(contact.conversations[idx].id);
			}
		} catch (e) {
			console.error('Failed to load contact:', e);
		}
	}

	async function loadConversationMessages(convId: string) {
		loadingMessages = true;
		try {
			messages = await listMessages(convId, undefined, 50);
			// Mark unread messages as read
			for (const m of messages) {
				if (m.read_status === 'Unread') {
					markMessageRead(m.id).catch(() => {});
				}
			}
		} catch (e) {
			console.error('Failed to load messages:', e);
			messages = [];
		}
		loadingMessages = false;
	}

	function selectConversation(idx: number) {
		selectedConvIdx = idx;
		if (contact && contact.conversations[idx]) {
			loadConversationMessages(contact.conversations[idx].id);
		}
	}

	function close() {
		app.contactPanelState = null;
	}

	// Drag handlers
	function handleToolbarPointerDown(e: PointerEvent) {
		if (e.button !== 0) return;
		dragging = true;
		dragStart = { x: e.clientX, y: e.clientY };
		dragOriginal = { x: position.x, y: position.y };
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
	}

	function handleToolbarPointerMove(e: PointerEvent) {
		if (!dragging) return;
		position = {
			x: dragOriginal.x + (e.clientX - dragStart.x),
			y: dragOriginal.y + (e.clientY - dragStart.y)
		};
	}

	function handleToolbarPointerUp(e: PointerEvent) {
		dragging = false;
		(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
	}

	function formatTime(iso: string): string {
		return new Date(iso).toLocaleString(undefined, {
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		});
	}
</script>

{#if app.contactPanelState && contact}
	<div class="contact-panel" style="left: {position.x}px; top: {position.y}px;">
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="panel-toolbar"
			onpointerdown={handleToolbarPointerDown}
			onpointermove={handleToolbarPointerMove}
			onpointerup={handleToolbarPointerUp}
		>
			<div class="toolbar-left">
				<div class="contact-avatar">{contact.name.charAt(0).toUpperCase()}</div>
				<span class="contact-name">{contact.name}</span>
			</div>
			<button class="close-btn" onclick={close} onpointerdown={(e) => e.stopPropagation()}>&times;</button>
		</div>

		<!-- Addresses -->
		<div class="addresses">
			{#each contact.addresses as addr}
				<div class="addr-row">
					<span class="addr-channel">{addr.channel}</span>
					<span class="addr-value">{addr.address}</span>
				</div>
			{/each}
		</div>

		<!-- Conversation tabs -->
		{#if contact.conversations.length > 1}
			<div class="conv-tabs">
				{#each contact.conversations as conv, i}
					<button
						class="conv-tab"
						class:active={selectedConvIdx === i}
						onclick={() => selectConversation(i)}
					>
						{conv.title || conv.channel}
						{#if conv.unread_count > 0}
							<span class="conv-unread">{conv.unread_count}</span>
						{/if}
					</button>
				{/each}
			</div>
		{/if}

		<!-- Messages -->
		<div class="messages">
			{#if loadingMessages}
				<div class="loading">Loading messages...</div>
			{:else if messages.length === 0}
				<div class="loading">No messages</div>
			{:else}
				{#each messages as msg (msg.id)}
					<div class="msg" class:outbound={msg.direction === 'Outbound'}>
						<div class="msg-header">
							<span class="msg-sender">
								{msg.direction === 'Outbound' ? 'You' : contact.name}
							</span>
							<span class="msg-time">{formatTime(msg.sent_at)}</span>
						</div>
						{#if msg.subject}
							<div class="msg-subject">{msg.subject}</div>
						{/if}
						<div class="msg-body">{msg.body}</div>
					</div>
				{/each}
			{/if}
		</div>
	</div>
{/if}

<style>
	.contact-panel {
		position: fixed;
		width: 480px;
		height: 500px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 12px;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
		z-index: 90;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.panel-toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
		cursor: grab;
		user-select: none;
	}

	.panel-toolbar:active {
		cursor: grabbing;
	}

	.toolbar-left {
		display: flex;
		align-items: center;
		gap: 10px;
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
	}

	.contact-name {
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

	.addresses {
		padding: 8px 14px;
		border-bottom: 1px solid var(--border);
	}

	.addr-row {
		display: flex;
		gap: 8px;
		font-size: 0.8rem;
		padding: 2px 0;
	}

	.addr-channel {
		color: var(--text-muted);
		min-width: 50px;
	}

	.addr-value {
		color: var(--text-secondary);
	}

	.conv-tabs {
		display: flex;
		gap: 0;
		border-bottom: 1px solid var(--border);
		overflow-x: auto;
	}

	.conv-tab {
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--text-secondary);
		padding: 8px 14px;
		font-size: 0.8rem;
		cursor: pointer;
		white-space: nowrap;
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.conv-tab:hover {
		background: var(--bg-hover);
	}

	.conv-tab.active {
		border-bottom-color: var(--accent);
		color: var(--accent);
	}

	.conv-unread {
		background: var(--error, #ef4444);
		color: #fff;
		font-size: 0.65rem;
		padding: 1px 4px;
		border-radius: 8px;
	}

	.messages {
		flex: 1;
		overflow-y: auto;
		padding: 8px 14px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.msg {
		padding: 8px 10px;
		border-radius: 8px;
		background: rgba(255, 255, 255, 0.04);
		max-width: 90%;
	}

	.msg.outbound {
		align-self: flex-end;
		background: color-mix(in srgb, var(--prov-owned) 15%, transparent);
	}

	.msg-header {
		display: flex;
		justify-content: space-between;
		gap: 8px;
		margin-bottom: 4px;
	}

	.msg-sender {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--text-secondary);
	}

	.msg-time {
		font-size: 0.7rem;
		color: var(--text-muted);
	}

	.msg-subject {
		font-size: 0.8rem;
		font-weight: 500;
		color: var(--text-primary);
		margin-bottom: 4px;
	}

	.msg-body {
		font-size: 0.8rem;
		color: var(--text-primary);
		line-height: 1.4;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.loading {
		padding: 24px;
		color: var(--text-muted);
		font-size: 0.85rem;
		text-align: center;
	}
</style>
