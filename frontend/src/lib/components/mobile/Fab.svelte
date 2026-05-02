<script lang="ts">
	/** Floating "+" action button for the mobile shell.
	 *
	 *  Short tap  → context-aware primary action:
	 *               · at least one lane exists → new doc in the current lane
	 *               · no lanes → new lane
	 *  Long press → open the full five-option fan menu (lane / doc / message /
	 *               capture / secret)
	 */
	import { createDocument, createThread } from '$lib/api/commands';
	import { canvas, mobileCanvas, load as canvasLoad } from '$lib/stores/canvas.svelte';
	import { longPress } from '$lib/actions/longPress';

	let menuOpen = $state(false);
	let longPressWasFired = false; // suppress the click that follows a long-press release

	function openMenu() {
		longPressWasFired = true;
		menuOpen = true;
	}

	function close() {
		menuOpen = false;
	}

	function handleFabTap() {
		if (longPressWasFired) {
			longPressWasFired = false;
			return;
		}
		// Short tap: primary action for the current context
		if (canvas.threads.length > 0) {
			handleNewDoc();
		} else {
			handleNewLane();
		}
	}

	async function handleNewDoc() {
		close();
		const title = window.prompt('New document title');
		if (!title || !title.trim()) return;
		// Use the currently active lane rather than always thread[0]
		const threadId = canvas.threads[mobileCanvas.currentLaneIndex]?.id ?? canvas.threads[0]?.id;
		if (!threadId) {
			alert('Create a lane first.');
			return;
		}
		try {
			await createDocument(title.trim(), threadId);
			await canvasLoad();
		} catch (e) {
			console.error('createDocument failed', e);
			alert(`Couldn't create document: ${e}`);
		}
	}

	async function handleNewLane() {
		close();
		const name = window.prompt('New lane name');
		if (!name || !name.trim()) return;
		try {
			await createThread(name.trim(), '');
			await canvasLoad();
		} catch (e) {
			console.error('createThread failed', e);
			alert(`Couldn't create lane: ${e}`);
		}
	}

	function handleNewMessage() {
		close();
		// Phase 4: opens contact picker → message composer
		alert('Message composer arrives in Phase 4.');
	}

	function handleCapture() {
		close();
		// Phase 4: full-screen camera view → save as image doc + on-device OCR
		alert('Camera capture arrives in Phase 4.');
	}

	function handleSecret() {
		close();
		// Phase 5: biometric-gated secret entry
		alert('Biometric secrets arrive in Phase 5.');
	}
</script>

<!-- Backdrop intercepts taps when menu is open -->
{#if menuOpen}
	<button class="backdrop" onclick={close} aria-label="Close menu"></button>
{/if}

<div class="fab-stack" class:open={menuOpen}>
	{#if menuOpen}
		<div class="menu" role="menu">
			<button class="menu-item" role="menuitem" onclick={handleNewLane}>
				<span class="menu-icon">&#8597;</span><span>New lane</span>
			</button>
			<button class="menu-item" role="menuitem" onclick={handleNewDoc}>
				<span class="menu-icon">&#128196;</span><span>New doc</span>
			</button>
			<button class="menu-item" role="menuitem" onclick={handleNewMessage}>
				<span class="menu-icon">&#128172;</span><span>New message</span>
			</button>
			<button class="menu-item" role="menuitem" onclick={handleCapture}>
				<span class="menu-icon">&#128247;</span><span>Capture</span>
			</button>
			<button class="menu-item" role="menuitem" onclick={handleSecret}>
				<span class="menu-icon">&#128272;</span><span>Secret</span>
			</button>
		</div>
	{/if}

	<button
		class="fab"
		onclick={handleFabTap}
		use:longPress={{ onLongPress: openMenu, duration: 500 }}
		aria-label={menuOpen ? 'Close create menu' : 'Open create menu'}
	>
		<span class="plus" class:rotated={menuOpen}>+</span>
	</button>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.35);
		border: none;
		padding: 0;
		z-index: 100;
		cursor: default;
	}

	.fab-stack {
		position: fixed;
		left: 50%;
		bottom: calc(env(safe-area-inset-bottom, 0px) + 56px + 12px);
		transform: translateX(-50%);
		z-index: 110;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		pointer-events: none;
	}

	.fab-stack > * {
		pointer-events: auto;
	}

	.menu {
		display: flex;
		flex-direction: column;
		gap: 4px;
		align-items: stretch;
		min-width: 180px;
		background: var(--bg-panel, #22222a);
		border: 1px solid var(--border, #333);
		border-radius: 12px;
		padding: 8px;
		box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
	}

	.menu-item {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 12px;
		background: none;
		border: none;
		color: var(--text-primary, #e0e0e0);
		font-size: 0.9rem;
		font-family: inherit;
		text-align: left;
		border-radius: 8px;
		cursor: pointer;
	}
	.menu-item:active {
		background: var(--bg-hover, #2a2a32);
	}

	.menu-icon {
		font-size: 1.1rem;
		width: 24px;
		text-align: center;
	}

	.fab {
		width: 56px;
		height: 56px;
		border-radius: 50%;
		background: var(--accent, #f59e0b);
		color: var(--bg-primary, #1a1a20);
		border: none;
		font-size: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
		box-shadow: 0 6px 16px rgba(0, 0, 0, 0.45);
		touch-action: none; /* let longPress own the pointer events */
	}
	.fab:active {
		filter: brightness(0.9);
	}

	.plus {
		font-size: 1.8rem;
		font-weight: 300;
		line-height: 1;
		transition: transform 0.18s ease;
	}

	.plus.rotated {
		transform: rotate(45deg);
	}
</style>
