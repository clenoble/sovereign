<script lang="ts">
	/** Slim bottom-anchored taskbar for the mobile shell.
	 *
	 *  Five icons that toggle the same panel-visibility flags the desktop
	 *  taskbar uses (`app.searchVisible`, `app.inboxVisible`, etc.).
	 *  Phase 1: panels render via the existing components (which may not yet
	 *  be visually optimized for narrow viewports — Phase 3 polish).
	 *
	 *  Note: a horizontal gap is left in the middle to make room for the
	 *  centered floating "+" FAB which sits above this row.
	 */
	import { app } from '$lib/stores/app.svelte';
	import { contactsState } from '$lib/stores/contacts.svelte';
	import { unreviewedCount, loadPii, piiState } from '$lib/stores/pii.svelte';

	let totalUnread = $derived(
		contactsState.contacts.reduce((sum, c) => sum + c.unread_count, 0)
	);

	function toggleSearch() {
		app.searchVisible = !app.searchVisible;
	}
	function toggleInbox() {
		app.inboxVisible = !app.inboxVisible;
	}
	function toggleContacts() {
		// Phase 1: toggling the contact panel without a contactId opens
		// the most recent. For now treat as no-op placeholder.
		const first = contactsState.contacts[0];
		if (first) {
			app.contactPanelState = { contactId: first.id };
		}
	}
	function togglePii() {
		app.piiDashboardVisible = !app.piiDashboardVisible;
		if (app.piiDashboardVisible && !piiState.loaded) {
			loadPii();
		}
	}
	function toggleSettings() {
		app.settingsVisible = !app.settingsVisible;
	}
</script>

<nav class="mobile-taskbar" aria-label="Primary actions">
	<button class="tab" onclick={toggleSearch} aria-label="Search">
		<span class="icon">🔍</span>
		<span class="label">Search</span>
	</button>

	<button class="tab" onclick={toggleInbox} aria-label="Inbox">
		<span class="icon">📥</span>
		<span class="label">Inbox</span>
	</button>

	<!-- Center gap reserved for floating + FAB. Empty grid cell — no content. -->
	<div aria-hidden="true"></div>

	<button class="tab" onclick={toggleContacts} aria-label="Contacts">
		<span class="icon">👥</span>
		<span class="label">People</span>
		{#if totalUnread > 0}
			<span class="badge">{totalUnread > 99 ? '99+' : totalUnread}</span>
		{/if}
	</button>

	<button class="tab" onclick={togglePii} aria-label="PII dashboard">
		<span class="icon">🔒</span>
		<span class="label">PII</span>
		{#if unreviewedCount() > 0}
			<span class="badge">{unreviewedCount() > 99 ? '99+' : unreviewedCount()}</span>
		{/if}
	</button>

	<button class="tab" onclick={toggleSettings} aria-label="Settings">
		<span class="icon">⚙️</span>
		<span class="label">Settings</span>
	</button>
</nav>

<style>
	.mobile-taskbar {
		flex-shrink: 0;
		display: grid;
		grid-template-columns: 1fr 1fr 64px 1fr 1fr 1fr;
		align-items: center;
		gap: 0;
		padding: 4px 0 max(env(safe-area-inset-bottom), 4px);
		background: var(--bg-panel, #22222a);
		border-top: 1px solid var(--border, #333);
	}

	.tab {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 2px;
		background: none;
		border: none;
		color: var(--text-muted, #888);
		font-size: 0.65rem;
		padding: 6px 2px;
		cursor: pointer;
		position: relative;
		min-height: 48px;
	}

	.tab:active {
		background: var(--bg-hover, #2a2a32);
	}

	.icon {
		font-size: 1.1rem;
		line-height: 1;
	}

	.label {
		font-size: 0.6rem;
	}

	.badge {
		position: absolute;
		top: 4px;
		right: calc(50% - 18px);
		background: var(--accent, #f59e0b);
		color: var(--bg-primary, #1a1a20);
		font-size: 0.55rem;
		font-weight: 700;
		padding: 1px 5px;
		border-radius: 8px;
		min-width: 14px;
		text-align: center;
	}

</style>
