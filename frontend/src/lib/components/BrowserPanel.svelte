<script lang="ts">
	import { onMount } from 'svelte';
	import {
		browser,
		closeBrowser,
		setBrowserAssessing,
		setBrowserReliability,
		updateBrowserBounds
	} from '$lib/stores/browser.svelte';
	import {
		openBrowser as openBrowserCmd,
		closeBrowserCmd,
		navigateBrowser,
		browserBack,
		browserForward,
		browserRefresh,
		setBrowserBounds,
		setBrowserVisible,
		assessReliability,
		saveWebPage
	} from '$lib/api/commands';
	import { refresh as canvasRefresh } from '$lib/stores/canvas.svelte';

	let urlInput = $state(browser.url || 'https://www.google.com');
	let webviewRegion: HTMLDivElement | undefined = $state();
	let resizeObserver: ResizeObserver | undefined;

	// Sync URL bar when navigation events update browser.url
	$effect(() => {
		if (browser.url && browser.url !== urlInput) {
			urlInput = browser.url;
		}
	});

	onMount(() => {
		// Measure the webview region and open the browser
		if (webviewRegion) {
			measureAndSync();
			resizeObserver = new ResizeObserver(() => measureAndSync());
			resizeObserver.observe(webviewRegion);
		}
		return () => {
			resizeObserver?.disconnect();
		};
	});

	async function measureAndSync() {
		if (!webviewRegion) return;
		const rect = webviewRegion.getBoundingClientRect();
		const bounds = {
			x: rect.left,
			y: rect.top,
			width: rect.width,
			height: rect.height
		};
		updateBrowserBounds(bounds);
		try {
			if (!browser.url) return;
			await openBrowserCmd(browser.url, bounds);
		} catch (e) {
			console.error('Failed to open browser:', e);
		}
	}

	async function handleNavigate() {
		let url = urlInput.trim();
		if (!url) return;
		// Auto-add protocol if missing
		if (!/^https?:\/\//.test(url)) {
			// If it looks like a search query, use Google
			if (!url.includes('.') || url.includes(' ')) {
				url = `https://www.google.com/search?q=${encodeURIComponent(url)}`;
			} else {
				url = `https://${url}`;
			}
		}
		urlInput = url;
		browser.url = url;
		browser.loading = true;
		try {
			await navigateBrowser(url);
		} catch (e) {
			console.error('Navigation failed:', e);
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			handleNavigate();
		}
	}

	async function handleBack() {
		try { await browserBack(); } catch { /* ignore */ }
	}
	async function handleForward() {
		try { await browserForward(); } catch { /* ignore */ }
	}
	async function handleRefresh() {
		try { await browserRefresh(); } catch { /* ignore */ }
	}

	async function handleClose() {
		closeBrowser();
		try { await closeBrowserCmd(); } catch { /* ignore */ }
	}

	async function handleAssess() {
		if (!browser.extractedText || browser.assessing) return;
		setBrowserAssessing(true);
		try {
			const result = await assessReliability(browser.extractedText);
			setBrowserReliability(result);
		} catch (e) {
			console.error('Assessment failed:', e);
			setBrowserAssessing(false);
		}
	}

	async function handleSave() {
		try {
			const r = browser.reliability;
			await saveWebPage(
				browser.url,
				browser.title || 'Untitled Page',
				browser.extractedText,
				undefined,
				r?.classification,
				r?.final_score,
				r ? JSON.stringify(r.raw_assessment) : undefined
			);
			canvasRefresh();
		} catch (e) {
			console.error('Save failed:', e);
		}
	}

	function scoreClass(score: number): string {
		if (score >= 3.5) return 'high';
		if (score >= 2.0) return 'medium';
		return 'low';
	}
</script>

<div class="browser-panel">
	<!-- Toolbar -->
	<div class="browser-toolbar">
		<button class="nav-btn" onclick={handleBack} title="Back">&#9664;</button>
		<button class="nav-btn" onclick={handleForward} title="Forward">&#9654;</button>
		<button class="nav-btn" onclick={handleRefresh} title="Refresh">&#8635;</button>
		<input
			class="url-bar"
			type="text"
			bind:value={urlInput}
			onkeydown={handleKeydown}
			placeholder="Enter URL or search..."
		/>
		<button class="nav-btn go-btn" onclick={handleNavigate}>Go</button>
		<button class="nav-btn close-btn" onclick={handleClose} title="Close browser">&times;</button>
	</div>

	<!-- Webview region (Tauri renders the native webview here) -->
	<div class="webview-region" bind:this={webviewRegion}></div>

	<!-- Reliability bar -->
	<div class="reliability-bar">
		{#if browser.reliability}
			<span class="classification">{browser.reliability.classification}</span>
			<span class="score-pill {scoreClass(browser.reliability.final_score)}">
				{browser.reliability.final_score.toFixed(1)}/5
			</span>
		{:else if browser.assessing}
			<span class="assessing">Assessing...</span>
		{/if}

		<div class="bar-actions">
			<button
				class="bar-btn"
				onclick={handleAssess}
				disabled={!browser.extractedText || browser.assessing}
			>Assess</button>
			<button
				class="bar-btn"
				onclick={handleSave}
				disabled={!browser.extractedText}
			>Save to Sovereign</button>
		</div>
	</div>
</div>

<style>
	.browser-panel {
		display: flex;
		flex-direction: column;
		width: 50%;
		min-width: 400px;
		max-width: 70%;
		border-left: 1px solid var(--border);
		background: var(--bg-secondary);
	}

	.browser-toolbar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 6px 8px;
		background: var(--bg-panel);
		border-bottom: 1px solid var(--border);
	}

	.nav-btn {
		background: none;
		border: 1px solid var(--border);
		color: var(--text-secondary);
		cursor: pointer;
		padding: 4px 8px;
		border-radius: 4px;
		font-size: 0.8rem;
	}
	.nav-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.go-btn {
		font-weight: 600;
		color: var(--accent);
		border-color: var(--accent);
	}

	.close-btn {
		font-size: 1.1rem;
		color: var(--text-muted);
		margin-left: auto;
	}

	.url-bar {
		flex: 1;
		padding: 5px 10px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--bg-input);
		color: var(--text-primary);
		font-size: 0.8rem;
		outline: none;
	}
	.url-bar:focus {
		border-color: var(--accent);
	}

	.webview-region {
		flex: 1;
		min-height: 200px;
		background: var(--bg-primary);
	}

	.reliability-bar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		background: var(--bg-panel);
		border-top: 1px solid var(--border);
		font-size: 0.75rem;
	}

	.classification {
		font-weight: 600;
		color: var(--text-primary);
	}

	.score-pill {
		padding: 2px 8px;
		border-radius: 10px;
		font-weight: 700;
		font-size: 0.7rem;
	}
	.score-pill.high {
		color: var(--reliability-high);
		background: var(--reliability-high-bg);
	}
	.score-pill.medium {
		color: var(--reliability-medium);
		background: var(--reliability-medium-bg);
	}
	.score-pill.low {
		color: var(--reliability-low);
		background: var(--reliability-low-bg);
	}

	.assessing {
		color: var(--text-muted);
		font-style: italic;
	}

	.bar-actions {
		margin-left: auto;
		display: flex;
		gap: 6px;
	}

	.bar-btn {
		padding: 3px 10px;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--bg-tertiary);
		color: var(--text-secondary);
		font-size: 0.7rem;
		cursor: pointer;
	}
	.bar-btn:hover:not(:disabled) {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	.bar-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
</style>
