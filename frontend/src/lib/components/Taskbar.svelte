<script lang="ts">
	import { app } from '$lib/stores/app.svelte';
	import { toggleChat } from '$lib/stores/chat.svelte';
	import { theme, applyTheme } from '$lib/stores/theme.svelte';
	import { toggleTheme as toggleThemeCmd } from '$lib/api/commands';
	import { canvas, navigateToDoc as canvasNavigateToDoc } from '$lib/stores/canvas.svelte';
	import { openById } from '$lib/stores/documents.svelte';
	import { contactsState } from '$lib/stores/contacts.svelte';

	function handleModels() {
		app.modelPanelVisible = !app.modelPanelVisible;
	}

	async function handleThemeToggle() {
		try {
			const next = await toggleThemeCmd();
			applyTheme(next as 'dark' | 'light');
		} catch {
			const next = theme.current === 'dark' ? 'light' : 'dark';
			applyTheme(next);
		}
	}

	function handleSearch() {
		app.searchVisible = !app.searchVisible;
	}

	function handleSettings() {
		app.settingsVisible = !app.settingsVisible;
	}

	function handleChat() {
		toggleChat();
	}

	function handleInbox() {
		app.inboxVisible = !app.inboxVisible;
	}

	function openDoc(id: string) {
		openById(id);
	}

	function navigateToDoc(id: string) {
		canvasNavigateToDoc(id);
	}

	function openContact(id: string) {
		app.contactPanelState = { contactId: id };
	}

	// Recent docs: up to 5 most recently modified
	let recentDocs = $derived(
		canvas.documents
			.slice()
			.sort((a: { modified_at: string }, b: { modified_at: string }) => new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime())
			.slice(0, 5)
	);

	// Recent contacts: up to 3 with most unread
	let recentContacts = $derived(contactsState.contacts.slice(0, 3));

	let totalUnread = $derived(contactsState.contacts.reduce((sum, c) => sum + c.unread_count, 0));
</script>

<nav class="taskbar">
	<div class="left">
		{#each recentDocs as doc (doc.id)}
			<button
				class="pinned-item"
				class:owned={doc.is_owned}
				class:external={!doc.is_owned}
				onclick={() => navigateToDoc(doc.id)}
				ondblclick={() => openDoc(doc.id)}
				title="{doc.title} (click: navigate, dbl-click: open)"
			>
				<span class="pin-label">{doc.title}</span>
			</button>
		{/each}
	</div>

	<div class="center">
		{#each recentContacts as contact (contact.id)}
			<button
				class="pinned-contact"
				onclick={() => openContact(contact.id)}
				title={contact.name}
			>
				<span class="contact-initial">{contact.name.charAt(0).toUpperCase()}</span>
				{#if contact.unread_count > 0}
					<span class="contact-badge">{contact.unread_count}</span>
				{/if}
			</button>
		{/each}
	</div>

	<div class="right">
		<button class="tb-btn" onclick={handleInbox} title="Inbox (I)">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
				<rect x="2" y="3" width="12" height="10" rx="2" stroke="currentColor" stroke-width="1.5" />
				<path d="M2 6 L8 10 L14 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
			</svg>
			{#if totalUnread > 0}
				<span class="unread-dot"></span>
			{/if}
		</button>

		<button class="tb-btn" onclick={handleSearch} title="Search (Ctrl+F)">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
				<circle cx="7" cy="7" r="5" stroke="currentColor" stroke-width="1.5" />
				<line x1="11" y1="11" x2="14" y2="14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
			</svg>
		</button>

		<button class="tb-btn" onclick={handleModels} title="Models">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
				<rect x="2" y="3" width="12" height="3" rx="1" stroke="currentColor" stroke-width="1.5" />
				<rect x="2" y="8" width="12" height="3" rx="1" stroke="currentColor" stroke-width="1.5" />
				<circle cx="5" cy="4.5" r="0.8" fill="currentColor" />
				<circle cx="5" cy="9.5" r="0.8" fill="currentColor" />
			</svg>
		</button>

		<button class="tb-btn" onclick={handleSettings} title="Settings">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
				<circle cx="8" cy="8" r="2.5" stroke="currentColor" stroke-width="1.5" />
				<path d="M8 1 V3 M8 13 V15 M1 8 H3 M13 8 H15 M2.9 2.9 L4.3 4.3 M11.7 11.7 L13.1 13.1 M13.1 2.9 L11.7 4.3 M4.3 11.7 L2.9 13.1" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
			</svg>
		</button>

		<button class="tb-btn" onclick={handleChat} title="Chat">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
				<rect x="1" y="2" width="14" height="10" rx="2" stroke="currentColor" stroke-width="1.5" />
				<path d="M5 14 L8 12 L3 12 Z" fill="currentColor" />
			</svg>
		</button>

		<button class="tb-btn" onclick={handleThemeToggle} title="Toggle theme">
			{#if theme.current === 'dark'}
				<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
					<circle cx="8" cy="8" r="4" stroke="currentColor" stroke-width="1.5" />
					<g stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
						<line x1="8" y1="1" x2="8" y2="3" />
						<line x1="8" y1="13" x2="8" y2="15" />
						<line x1="1" y1="8" x2="3" y2="8" />
						<line x1="13" y1="8" x2="15" y2="8" />
					</g>
				</svg>
			{:else}
				<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
					<path d="M13 9 A5 5 0 1 1 7 3 A4 4 0 0 0 13 9 Z" stroke="currentColor" stroke-width="1.5" />
				</svg>
			{/if}
		</button>
	</div>
</nav>

<style>
	.taskbar {
		position: fixed;
		bottom: 0;
		left: 0;
		right: 0;
		height: 44px;
		background: var(--bg-secondary);
		border-top: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 16px;
		z-index: 80;
	}

	.left,
	.center,
	.right {
		display: flex;
		align-items: center;
	}

	.left {
		flex: 1;
		overflow: hidden;
		gap: 2px;
	}

	.center {
		gap: 6px;
		padding: 0 12px;
	}

	.right {
		gap: 4px;
	}

	/* Pinned document items */
	.pinned-item {
		display: flex;
		align-items: center;
		padding: 4px 10px;
		border-radius: 4px;
		border: 1px solid transparent;
		background: none;
		color: var(--text-secondary);
		font-size: 0.75rem;
		cursor: pointer;
		max-width: 120px;
		overflow: hidden;
		white-space: nowrap;
	}
	.pinned-item:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	.pinned-item.owned {
		border-left: 2px solid #5a9fd4;
	}
	.pinned-item.external {
		border-left: 2px solid #e07c6a;
	}
	.pin-label {
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* Pinned contact items */
	.pinned-contact {
		position: relative;
		width: 28px;
		height: 28px;
		border-radius: 50%;
		border: 1px solid var(--border);
		background: var(--bg-hover);
		color: var(--text-secondary);
		font-size: 0.7rem;
		font-weight: 600;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.pinned-contact:hover {
		border-color: var(--accent);
		color: var(--text-primary);
	}
	.contact-badge {
		position: absolute;
		top: -4px;
		right: -4px;
		min-width: 14px;
		height: 14px;
		border-radius: 7px;
		background: var(--error, #ef4444);
		color: #fff;
		font-size: 0.6rem;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0 3px;
	}

	.tb-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		padding: 6px;
		border-radius: 4px;
		display: flex;
		align-items: center;
		justify-content: center;
		position: relative;
	}

	.tb-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.unread-dot {
		position: absolute;
		top: 4px;
		right: 4px;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--error, #ef4444);
	}
</style>
