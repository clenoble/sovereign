<script lang="ts">
	import { onMount } from 'svelte';
	import { applyTheme } from '$lib/stores/theme';
	import { searchVisible } from '$lib/stores/app';
	import { subscribeToEvents } from '$lib/api/events';
	import { getTheme } from '$lib/api/commands';

	import Taskbar from '$lib/components/Taskbar.svelte';
	import Bubble from '$lib/components/Bubble.svelte';
	import Chat from '$lib/components/Chat.svelte';
	import Search from '$lib/components/Search.svelte';
	import ConfirmAction from '$lib/components/ConfirmAction.svelte';

	let { children } = $props();

	onMount(async () => {
		// Apply initial theme from backend
		try {
			const theme = await getTheme();
			applyTheme(theme as 'dark' | 'light');
		} catch {
			applyTheme('dark');
		}

		// Subscribe to backend events
		const unlisten = await subscribeToEvents();

		// Global keyboard shortcuts
		const handleKeydown = (e: KeyboardEvent) => {
			if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
				e.preventDefault();
				searchVisible.update((v) => !v);
			}
		};
		window.addEventListener('keydown', handleKeydown);

		return () => {
			unlisten();
			window.removeEventListener('keydown', handleKeydown);
		};
	});
</script>

<svelte:head>
	<title>Sovereign GE</title>
</svelte:head>

<div class="app">
	{@render children()}
	<Bubble />
	<Chat />
	<Search />
	<ConfirmAction />
	<Taskbar />
</div>

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
</style>
