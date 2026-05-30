<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { applyTheme } from '$lib/stores/theme.svelte';
	import { app } from '$lib/stores/app.svelte';
	import { toggleChat } from '$lib/stores/chat.svelte';
	import { subscribeToEvents } from '$lib/api/events';
	import { getTheme, checkAuthState, getProfile, triggerSyncNow } from '$lib/api/commands';
	import { stopNowTimer } from '$lib/stores/canvas.svelte';
	import { device, initDevice, destroyDevice } from '$lib/stores/device.svelte';

	import Taskbar from '$lib/components/Taskbar.svelte';
	import Bubble from '$lib/components/Bubble.svelte';
	import Chat from '$lib/components/Chat.svelte';
	import Search from '$lib/components/Search.svelte';
	import ConfirmAction from '$lib/components/ConfirmAction.svelte';
	import ModelPanel from '$lib/components/ModelPanel.svelte';
	import InboxPanel from '$lib/components/InboxPanel.svelte';
	import ContactPanel from '$lib/components/ContactPanel.svelte';
	import PiiDashboardPanel from '$lib/components/PiiDashboardPanel.svelte';
	import SignupCapturePrompt from '$lib/components/SignupCapturePrompt.svelte';
	import AutofillPrompt from '$lib/components/AutofillPrompt.svelte';
	import ContextMenu from '$lib/components/ContextMenu.svelte';
	import { piiState } from '$lib/stores/pii.svelte';
	import { listen } from '@tauri-apps/api/event';
	import type { BrowserFormExtraction } from '$lib/api/commands';
	import LoginScreen from '$lib/components/LoginScreen.svelte';
	import OnboardingWizard from '$lib/components/OnboardingWizard.svelte';
	import SettingsPanel from '$lib/components/SettingsPanel.svelte';
	import MobileShell from '$lib/components/mobile/MobileShell.svelte';

	let { children } = $props();

	// Cleanup is assigned at the end of the async onMount body. Svelte
	// ignores a Promise returned from an async onMount, so the teardown
	// is registered via onDestroy instead of `return () => …`.
	let cleanup: (() => void) | null = null;
	onDestroy(() => cleanup?.());

	onMount(async () => {
		// Initialize device detection (viewport + platform). Subscribes
		// to resize so toggling Chrome devtools' device emulation flips
		// the layout live.
		initDevice();

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

		// Global keyboard shortcuts (desktop only — mobile has no
		// physical keyboard and these would compete with platform IME).
		const handleKeydown = (e: KeyboardEvent) => {
			if (device.isMobile) return;
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
				// PII dashboard's Escape is handled by its focusTrap when
				// focus is inside the panel; this fallback covers the case
				// where focus escaped the panel (e.g. clicked outside).
				if (app.piiDashboardVisible) { app.piiDashboardVisible = false; return; }
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

			// P: toggle PII dashboard
			if (e.key === 'p' || e.key === 'P') {
				app.piiDashboardVisible = !app.piiDashboardVisible;
			}
		};
		window.addEventListener('keydown', handleKeydown);

		// Listen for the browser-form-extracted event triggered by
		// the BrowserPanel's "Save credentials" / "Fill from vault"
		// buttons. The dispatch is by the autofillRequested flag —
		// both buttons share the same Tauri command + event but the
		// flag tells us which dialog to open.
		const unlistenSignup = await listen<BrowserFormExtraction>(
			'browser-form-extracted',
			(event) => {
				if (piiState.autofillRequested) {
					piiState.autofillExtraction = event.payload;
					piiState.autofillRequested = false;
				} else {
					piiState.signupCapture = event.payload;
				}
			}
		);

		// Phase 3c: foreground sync trigger. When the document becomes
		// visible (window refocus, mobile resume from background), kick
		// off a sync with every paired peer. The backend dedupes against
		// in-flight sessions per peer, so a quick toggle won't pile up
		// duplicate syncs.
		let lastVisibilitySync = 0;
		const handleVisibilityChange = () => {
			if (document.visibilityState !== 'visible') return;
			// 60s cooldown per the v0.0.5 plan §4.5 (foreground budget).
			const now = Date.now();
			if (now - lastVisibilitySync < 60_000) return;
			lastVisibilitySync = now;
			triggerSyncNow().catch((e) => console.warn('triggerSyncNow failed:', e));
		};
		document.addEventListener('visibilitychange', handleVisibilityChange);

		cleanup = () => {
			unlisten();
			unlistenSignup();
			stopNowTimer();
			destroyDevice();
			window.removeEventListener('keydown', handleKeydown);
			document.removeEventListener('visibilitychange', handleVisibilityChange);
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
		{#if device.isMobile}
			<MobileShell />
		{:else}
			{@render children()}
			<Bubble />
			<Taskbar />
		{/if}

		<!-- Chat is rendered inside MobileChatSheet on mobile (bottom sheet).
		     On desktop it stays as the floating panel. -->
		{#if !device.isMobile}
			<Chat />
		{/if}
		<Search />
		<ConfirmAction />
		<ModelPanel />
		<InboxPanel />
		<ContactPanel />
		<PiiDashboardPanel />
		<SignupCapturePrompt
			open={piiState.signupCapture !== null}
			extraction={piiState.signupCapture}
			onClose={() => (piiState.signupCapture = null)}
		/>
		<AutofillPrompt
			open={piiState.autofillExtraction !== null}
			extraction={piiState.autofillExtraction}
			onClose={() => (piiState.autofillExtraction = null)}
		/>
		<ContextMenu />
		<SettingsPanel />
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
