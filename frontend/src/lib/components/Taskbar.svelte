<script lang="ts">
	import { searchVisible } from '$lib/stores/app';
	import { chat } from '$lib/stores/chat';
	import { currentTheme, applyTheme } from '$lib/stores/theme';
	import { toggleTheme as toggleThemeCmd } from '$lib/api/commands';

	async function handleThemeToggle() {
		try {
			const next = await toggleThemeCmd();
			applyTheme(next as 'dark' | 'light');
		} catch {
			// Fallback: toggle locally
			const next = $currentTheme === 'dark' ? 'light' : 'dark';
			applyTheme(next);
		}
	}

	function handleSearch() {
		searchVisible.update((v) => !v);
	}

	function handleChat() {
		chat.toggle();
	}
</script>

<nav class="taskbar">
	<div class="left">
		<span class="brand">Sovereign GE</span>
	</div>

	<div class="center">
		<!-- Pinned items will go here in Phase 2+ -->
	</div>

	<div class="right">
		<button class="tb-btn" onclick={handleSearch} title="Search (Ctrl+F)">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
				<circle cx="7" cy="7" r="5" stroke="currentColor" stroke-width="1.5" />
				<line x1="11" y1="11" x2="14" y2="14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
			</svg>
		</button>

		<button class="tb-btn" onclick={handleChat} title="Chat">
			<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
				<rect x="1" y="2" width="14" height="10" rx="2" stroke="currentColor" stroke-width="1.5" />
				<path d="M5 14 L8 12 L3 12 Z" fill="currentColor" />
			</svg>
		</button>

		<button class="tb-btn" onclick={handleThemeToggle} title="Toggle theme">
			{#if $currentTheme === 'dark'}
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

	.brand {
		font-size: 0.8rem;
		font-weight: 600;
		color: var(--accent);
		letter-spacing: 0.05em;
	}

	.left,
	.center,
	.right {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.center {
		flex: 1;
		justify-content: center;
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
	}

	.tb-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
</style>
