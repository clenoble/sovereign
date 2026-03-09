<script lang="ts">
	import { onMount } from 'svelte';
	import { applyTheme } from '$lib/stores/theme.svelte';
	import { app } from '$lib/stores/app.svelte';
	import { toggleChat } from '$lib/stores/chat.svelte';
	import { subscribeToEvents } from '$lib/api/events';
	import { getTheme, checkAuthState, getProfile } from '$lib/api/commands';
	import { stopNowTimer } from '$lib/stores/canvas.svelte';

	import Taskbar from '$lib/components/Taskbar.svelte';
	import Bubble from '$lib/components/Bubble.svelte';
	import Chat from '$lib/components/Chat.svelte';
	import Search from '$lib/components/Search.svelte';
	import ConfirmAction from '$lib/components/ConfirmAction.svelte';
	import ModelPanel from '$lib/components/ModelPanel.svelte';
	import InboxPanel from '$lib/components/InboxPanel.svelte';
	import ContactPanel from '$lib/components/ContactPanel.svelte';
	import ContextMenu from '$lib/components/ContextMenu.svelte';
	import LoginScreen from '$lib/components/LoginScreen.svelte';
	import OnboardingWizard from '$lib/components/OnboardingWizard.svelte';
	import SettingsPanel from '$lib/components/SettingsPanel.svelte';

	let { children } = $props();

	onMount(async () => {
		// Check auth state first
		try {
			const auth = await checkAuthState();
			if (auth.needs_onboarding) {
				app.authState = 'onboarding';
			} else if (auth.needs_login) {
				app.authState = 'login';
			} else {
				app.authState = 'ready';
			}
		} catch {
			// Backend not ready yet — assume ready (no auth)
			app.authState = 'ready';
		}

		// Apply initial theme from backend
		try {
			const t = await getTheme();
			applyTheme(t as 'dark' | 'light');
		} catch {
			applyTheme('dark');
		}

		// Load user profile for bubble style
		try {
			const profile = await getProfile();
			if (profile.bubble_style) app.bubbleStyle = profile.bubble_style;
		} catch { /* profile not available yet */ }

		// Subscribe to backend events
		const unlisten = await subscribeToEvents();

		// Global keyboard shortcuts
		const handleKeydown = (e: KeyboardEvent) => {
			const tag = (e.target as HTMLElement)?.tagName;
			const editable = (e.target as HTMLElement)?.isContentEditable;
			const isInput = tag === 'INPUT' || tag === 'TEXTAREA' || editable;

			// Escape: close topmost overlay
			if (e.key === 'Escape') {
				if (app.contextMenu) { app.contextMenu = null; return; }
				if (app.searchVisible) { app.searchVisible = false; return; }
				if (app.settingsVisible) { app.settingsVisible = false; return; }
				if (app.modelPanelVisible) { app.modelPanelVisible = false; return; }
				if (app.inboxVisible) { app.inboxVisible = false; return; }
				if (app.contactPanelState) { app.contactPanelState = null; return; }
				if (app.skillsPanelVisible) { app.skillsPanelVisible = false; return; }
				return;
			}

			// Ctrl+F: toggle search
			if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
				e.preventDefault();
				app.searchVisible = !app.searchVisible;
				return;
			}

			// Ctrl+N: new document (open chat)
			if ((e.ctrlKey || e.metaKey) && e.key === 'n') {
				e.preventDefault();
				toggleChat();
				return;
			}

			// Skip non-Escape shortcuts when in input
			if (isInput) return;

			// I: toggle inbox
			if (e.key === 'i' || e.key === 'I') {
				app.inboxVisible = !app.inboxVisible;
			}
		};
		window.addEventListener('keydown', handleKeydown);

		return () => {
			unlisten();
			stopNowTimer();
			window.removeEventListener('keydown', handleKeydown);
		};
	});
</script>

<svelte:head>
	<title>Sovereign GE</title>
</svelte:head>

{#if app.authState === 'checking'}
	<div class="loading">
		<span class="spinner"></span>
		<span>Loading...</span>
	</div>
{:else if app.authState === 'onboarding'}
	<OnboardingWizard />
{:else if app.authState === 'login'}
	<LoginScreen />
{:else}
	<div class="app">
		{@render children()}
		<Bubble />
		<Chat />
		<Search />
		<ConfirmAction />
		<ModelPanel />
		<InboxPanel />
		<ContactPanel />
		<ContextMenu />
		<SettingsPanel />
		<Taskbar />
	</div>
{/if}

<style>
	:global(body) {
		margin: 0;
		padding: 0;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
		background-color: var(--bg-primary, #1a1a20);
		color: var(--text-primary, #e0e0e0);
		overflow: hidden;
	}

	:global(*) {
		box-sizing: border-box;
	}

	.app {
		width: 100vw;
		height: 100vh;
		display: flex;
		flex-direction: column;
	}

	.loading {
		width: 100vw;
		height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 12px;
		color: var(--text-muted, #888);
		font-size: 0.9rem;
	}

	.spinner {
		width: 20px;
		height: 20px;
		border: 2px solid var(--border, #333);
		border-top-color: var(--accent, #F59E0B);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
