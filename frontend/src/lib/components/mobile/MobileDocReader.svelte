<script lang="ts">
	/** Full-screen document reader/editor for mobile.
	 *
	 *  Replaces the desktop floating DocumentPanel on mobile.
	 *  Swipe-down on the top bar (> 80px travel) closes the reader.
	 *  Modes: 'edit' (textarea), 'preview' (rendered markdown) and 'history'
	 *  (the versions, with "Use this version": the ordinary version restore,
	 *  C-WIRE 14 — how a change from another device is undone on mobile).
	 *  Auto-saves 2 s after each edit — same policy as DocumentPanel.
	 */
	import { renderMarkdown } from '$lib/utils/markdown';
	import {
		type OpenPanel,
		save,
		close,
		updateBody,
		updateTitle,
		setMode,
		loadCommits,
		selectCommit,
		restoreVersion
	} from '$lib/stores/documents.svelte';
	import SaveNotice from '../SaveNotice.svelte';

	let { panel }: { panel: OpenPanel } = $props();

	let saveTimer: ReturnType<typeof setTimeout> | null = null;

	// Swipe-down-to-dismiss state
	let dragStartY = 0;
	let dragDy = $state(0);
	let dragging = $state(false);

	function onHeaderPointerDown(e: PointerEvent) {
		dragging = true;
		dragStartY = e.clientY;
		dragDy = 0;
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
	}

	function onHeaderPointerMove(e: PointerEvent) {
		if (!dragging) return;
		const dy = e.clientY - dragStartY;
		dragDy = dy > 0 ? dy : 0; // only downward drag
	}

	async function onHeaderPointerUp(e: PointerEvent) {
		if (!dragging) return;
		dragging = false;
		const dy = e.clientY - dragStartY;
		if (dy > 80) {
			await handleClose();
		} else {
			dragDy = 0;
		}
	}

	function scheduleSave() {
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = setTimeout(() => save(panel.doc.id), 2000);
	}

	function handleBodyInput(e: Event) {
		updateBody(panel.doc.id, (e.target as HTMLTextAreaElement).value);
		scheduleSave();
	}

	function handleTitleInput(e: Event) {
		updateTitle(panel.doc.id, (e.target as HTMLInputElement).value);
		scheduleSave();
	}

	async function handleClose() {
		if (saveTimer) {
			clearTimeout(saveTimer);
			saveTimer = null;
		}
		dragDy = 0;
		await close(panel.doc.id);
	}

	function toggleMode() {
		setMode(panel.doc.id, panel.mode === 'edit' ? 'preview' : 'edit');
	}

	async function toggleHistory() {
		if (panel.mode === 'history') {
			setMode(panel.doc.id, 'edit');
			return;
		}
		setMode(panel.doc.id, 'history');
		await loadCommits(panel.doc.id);
	}
</script>

<div
	class="reader"
	style:transform="translateY({dragDy}px)"
	style:transition={dragging ? 'none' : 'transform 0.22s ease'}
	aria-modal="true"
	role="dialog"
	aria-label="Document: {panel.doc.title || 'Untitled'}"
>
	<!-- Drag-down-to-close top bar -->
	<div
		class="reader-header"
		onpointerdown={onHeaderPointerDown}
		onpointermove={onHeaderPointerMove}
		onpointerup={onHeaderPointerUp}
		onpointercancel={onHeaderPointerUp}
		role="button"
		tabindex="0"
		aria-label="Drag down to close"
	>
		<div class="drag-hint" aria-hidden="true"></div>

		<div class="header-row">
			{#if panel.mode === 'edit'}
				<input
					class="title-input"
					value={panel.doc.title}
					oninput={handleTitleInput}
					placeholder="Untitled"
					aria-label="Document title"
				/>
			{:else}
				<span class="title-text">{panel.doc.title || 'Untitled'}</span>
			{/if}

			<div class="header-actions">
				{#if panel.dirty}
					<span class="dirty-dot" title="Unsaved changes" aria-label="Unsaved changes">&#9679;</span>
				{/if}
				{#if panel.mode !== 'history'}
					<button class="mode-btn" onclick={toggleMode}>
						{panel.mode === 'edit' ? 'Preview' : 'Edit'}
					</button>
				{/if}
				<button class="mode-btn" onclick={toggleHistory} aria-pressed={panel.mode === 'history'}>
					{panel.mode === 'history' ? 'Back' : 'History'}
				</button>
				<button class="close-btn" onclick={handleClose} aria-label="Close document">&#10005;</button>
			</div>
		</div>
	</div>

	<SaveNotice {panel} />

	<!-- Document body -->
	<div class="reader-body">
		{#if panel.mode === 'history'}
			<div class="history">
				{#if panel.commits.length === 0}
					<p class="history-empty">No saved versions yet.</p>
				{:else}
					<ul class="commit-list" aria-label="Versions">
						{#each panel.commits as commit, i (commit.id)}
							<li>
								<button
									class="commit-item"
									class:selected={panel.selectedCommit === i}
									aria-pressed={panel.selectedCommit === i}
									onclick={() => selectCommit(panel.doc.id, i)}
								>
									<span class="commit-msg">{commit.message}</span>
									<span class="commit-time">{new Date(commit.timestamp).toLocaleString()}</span>
								</button>
							</li>
						{/each}
					</ul>
				{/if}
				{#if panel.selectedCommit !== null && panel.commits[panel.selectedCommit]}
					<div class="commit-preview">
						<h4>{panel.commits[panel.selectedCommit].snapshot_title}</h4>
						<p>{panel.commits[panel.selectedCommit].snapshot_preview}</p>
						<button
							class="use-btn"
							onclick={() => restoreVersion(panel.doc.id, panel.commits[panel.selectedCommit!].id)}
						>
							Use this version
						</button>
					</div>
				{/if}
			</div>
		{:else if panel.mode === 'preview'}
			<div class="preview">{@html renderMarkdown(panel.doc.body ?? '')}</div>
		{:else}
			<textarea
				class="editor"
				value={panel.doc.body ?? ''}
				oninput={handleBodyInput}
				placeholder="Start writing…"
				aria-label="Document body"
				spellcheck="true"
			></textarea>
		{/if}
	</div>
</div>

<style>
	.reader {
		position: fixed;
		inset: 0;
		background: var(--bg-primary, #1a1a20);
		display: flex;
		flex-direction: column;
		z-index: 200;
	}

	/* ── Header ──────────────────────────────────────────────── */
	.reader-header {
		flex-shrink: 0;
		padding-top: max(env(safe-area-inset-top, 0px), 6px);
		background: var(--bg-panel, #22222a);
		border-bottom: 1px solid var(--border, #333);
		cursor: grab;
		touch-action: none;
		user-select: none;
	}
	.reader-header:active { cursor: grabbing; }

	.drag-hint {
		width: 36px;
		height: 4px;
		border-radius: 2px;
		background: var(--text-muted, #555);
		opacity: 0.4;
		margin: 6px auto 0;
	}

	.header-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px 10px;
	}

	.title-input {
		flex: 1;
		min-width: 0;
		background: none;
		border: none;
		color: var(--text-primary, #e0e0e0);
		font-size: 1rem;
		font-weight: 600;
		outline: none;
		font-family: inherit;
	}

	.title-text {
		flex: 1;
		min-width: 0;
		font-size: 1rem;
		font-weight: 600;
		color: var(--text-primary, #e0e0e0);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.header-actions {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.dirty-dot {
		color: var(--accent, #f59e0b);
		font-size: 0.55rem;
	}

	.mode-btn {
		background: var(--bg-hover, #2a2a32);
		border: 1px solid var(--border, #333);
		border-radius: 6px;
		color: var(--text-secondary, #bbb);
		font-size: 0.8rem;
		padding: 5px 10px;
		cursor: pointer;
		font-family: inherit;
	}
	.mode-btn:active { filter: brightness(0.85); }

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted, #666);
		font-size: 1rem;
		padding: 4px 8px;
		cursor: pointer;
	}
	.close-btn:hover, .close-btn:active { color: var(--text-primary, #e0e0e0); }

	/* ── Body ────────────────────────────────────────────────── */
	.reader-body {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.editor {
		flex: 1;
		width: 100%;
		background: var(--bg-primary, #1a1a20);
		border: none;
		color: var(--text-primary, #e0e0e0);
		font-family: inherit;
		font-size: 0.92rem;
		line-height: 1.65;
		padding: 16px;
		resize: none;
		outline: none;
	}

	.preview {
		flex: 1;
		overflow-y: auto;
		padding: 16px;
		font-size: 0.92rem;
		line-height: 1.65;
		color: var(--text-primary, #e0e0e0);
	}
	.preview :global(p)    { margin: 8px 0; }
	.preview :global(h1), .preview :global(h2), .preview :global(h3) { margin: 16px 0 8px; }
	.preview :global(code) { background: rgba(255,255,255,0.08); padding: 1px 4px; border-radius: 3px; font-size: 0.85em; }
	.preview :global(pre)  { background: rgba(0,0,0,0.3); padding: 10px 12px; border-radius: 6px; overflow-x: auto; margin: 8px 0; }
	.preview :global(pre code) { background: none; padding: 0; }
	.preview :global(ul), .preview :global(ol) { padding-left: 20px; margin: 6px 0; }
	.preview :global(a) { color: var(--accent, #f59e0b); }

	/* ── History ─────────────────────────────────────────────── */
	.history {
		flex: 1;
		overflow-y: auto;
		padding: 12px;
	}
	.history-empty {
		color: var(--text-secondary, #c8c8c8);
	}
	.commit-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.commit-item {
		width: 100%;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 2px;
		padding: 10px 12px;
		min-height: 44px;
		background: var(--bg-panel, #22222a);
		color: var(--text-primary, #e0e0e0);
		border: 1px solid var(--border, #444);
		border-radius: 8px;
		text-align: left;
		font: inherit;
	}
	.commit-item.selected {
		border-color: #6366f1;
		box-shadow: 0 0 0 2px #6366f1 inset;
	}
	.commit-time {
		font-size: 0.8rem;
		color: var(--text-secondary, #c8c8c8);
	}
	.commit-preview {
		margin-top: 12px;
		padding: 12px;
		border: 1px solid var(--border, #444);
		border-radius: 8px;
	}
	.commit-preview h4 {
		margin: 0 0 6px;
	}
	.commit-preview p {
		margin: 0 0 10px;
		white-space: pre-wrap;
	}
	/* White on indigo-600 (#4f46e5): 6.3:1. */
	.use-btn {
		background: #4f46e5;
		color: #fff;
		border: none;
		border-radius: 8px;
		padding: 10px 16px;
		min-height: 44px;
		font: inherit;
	}
	.commit-item:focus-visible,
	.use-btn:focus-visible {
		outline: 3px solid #fbbf24;
		outline-offset: 2px;
	}
</style>
