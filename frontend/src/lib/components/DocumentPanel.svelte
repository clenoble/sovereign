<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { marked } from 'marked';
	import { type OpenPanel, save, updateBody, updateTitle, close, bringToFront, updatePosition, setMode, loadCommits, selectCommit, restoreVersion, toggleSkillsOverflow } from '$lib/stores/documents.svelte';
	import { listSkillsForDoc, executeSkill } from '$lib/api/commands';
	import { pushSystem } from '$lib/stores/chat.svelte';
	import type { SkillInfo, SkillResultDto } from '$lib/api/commands';

	let { panel }: { panel: OpenPanel } = $props();

	let skills = $state<SkillInfo[]>([]);
	let textarea: HTMLTextAreaElement | undefined = $state();
	let saveTimer: ReturnType<typeof setTimeout> | null = null;

	// Drag state
	let dragging = $state(false);
	let dragStart = { x: 0, y: 0 };
	let panelStart = { x: 0, y: 0 };

	// Load skills on mount + title change
	let lastTitle = '';
	async function refreshSkills() {
		if (panel.doc.title === lastTitle) return;
		lastTitle = panel.doc.title;
		try {
			skills = await listSkillsForDoc(panel.doc.title);
		} catch {
			skills = [];
		}
	}

	onMount(() => {
		refreshSkills();
	});

	onDestroy(() => {
		if (saveTimer) clearTimeout(saveTimer);
	});

	$effect(() => {
		// Re-fetch skills when title changes
		panel.doc.title;
		refreshSkills();
	});

	// Auto-save: debounce 2s after body/title edits
	function scheduleSave() {
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = setTimeout(() => {
			save(panel.doc.id);
		}, 2000);
	}

	function handleBodyInput(e: Event) {
		const val = (e.target as HTMLTextAreaElement).value;
		updateBody(panel.doc.id, val);
		scheduleSave();
	}

	function handleTitleInput(e: Event) {
		const val = (e.target as HTMLInputElement).value;
		updateTitle(panel.doc.id, val);
		scheduleSave();
	}

	function handleSave() {
		if (saveTimer) clearTimeout(saveTimer);
		save(panel.doc.id);
	}

	function handleClose() {
		if (saveTimer) clearTimeout(saveTimer);
		close(panel.doc.id);
	}

	// Keyboard shortcuts
	function handleKeydown(e: KeyboardEvent) {
		if ((e.ctrlKey || e.metaKey) && e.key === 's') {
			e.preventDefault();
			handleSave();
		} else if (e.key === 'Escape') {
			handleClose();
		}
	}

	// Drag handling on toolbar
	function handlePointerDown(e: PointerEvent) {
		// Only drag from the toolbar area, not buttons
		if ((e.target as HTMLElement).closest('button')) return;
		dragging = true;
		dragStart = { x: e.clientX, y: e.clientY };
		panelStart = { x: panel.position.x, y: panel.position.y };
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		bringToFront(panel.doc.id);
	}

	function handlePointerMove(e: PointerEvent) {
		if (!dragging) return;
		const dx = e.clientX - dragStart.x;
		const dy = e.clientY - dragStart.y;
		updatePosition(panel.doc.id, panelStart.x + dx, panelStart.y + dy);
	}

	function handlePointerUp() {
		dragging = false;
	}

	// Format toolbar — inserts markdown at cursor
	function insertFormat(prefix: string, suffix: string) {
		if (!textarea) return;
		const start = textarea.selectionStart;
		const end = textarea.selectionEnd;
		const text = panel.doc.body;
		const selected = text.slice(start, end);
		const replacement = prefix + (selected || 'text') + suffix;
		const newBody = text.slice(0, start) + replacement + text.slice(end);
		updateBody(panel.doc.id, newBody);
		scheduleSave();
		// Restore cursor after Svelte re-renders
		requestAnimationFrame(() => {
			if (!textarea) return;
			const cursorPos = start + prefix.length + (selected || 'text').length;
			textarea.selectionStart = cursorPos;
			textarea.selectionEnd = cursorPos;
			textarea.focus();
		});
	}

	// Mode toggles
	function togglePreview() {
		setMode(panel.doc.id, panel.mode === 'preview' ? 'edit' : 'preview');
	}

	function toggleHistory() {
		if (panel.mode === 'history') {
			setMode(panel.doc.id, 'edit');
		} else {
			setMode(panel.doc.id, 'history');
			loadCommits(panel.doc.id);
		}
	}

	// Skill execution
	async function runSkill(skillName: string, actionId: string) {
		try {
			const result: SkillResultDto = await executeSkill(
				skillName,
				actionId,
				panel.doc.id,
				'{}'
			);
			if (result.kind === 'content_update' && result.body !== undefined) {
				updateBody(panel.doc.id, result.body);
			} else if (result.kind === 'file' && result.file_data_base64) {
				// Trigger download
				const bytes = Uint8Array.from(atob(result.file_data_base64), (c) => c.charCodeAt(0));
				const blob = new Blob([bytes], { type: result.file_mime || 'application/octet-stream' });
				const url = URL.createObjectURL(blob);
				const a = document.createElement('a');
				a.href = url;
				a.download = result.file_name || 'download';
				a.click();
				URL.revokeObjectURL(url);
			} else if (result.kind === 'structured_data' && result.structured_json) {
				pushSystem(`Skill result (${result.structured_kind}): ${result.structured_json}`);
			}
		} catch (e) {
			pushSystem(`Skill error: ${e}`);
		}
	}

	// Derived markdown HTML
	let previewHtml = $derived(marked.parse(panel.doc.body || '') as string);

	// Flatten top 3 skill actions for quick-access buttons
	let quickActions = $derived.by(() => {
		const all: { skill: string; id: string; label: string }[] = [];
		for (const s of skills) {
			for (const a of s.actions) {
				all.push({ skill: s.skill_name, id: a.action_id, label: a.label });
			}
		}
		return all.slice(0, 3);
	});

	let overflowActions = $derived.by(() => {
		const all: { skill: string; id: string; label: string }[] = [];
		for (const s of skills) {
			for (const a of s.actions) {
				all.push({ skill: s.skill_name, id: a.action_id, label: a.label });
			}
		}
		return all.slice(3);
	});
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="doc-panel"
	style="left: {panel.position.x}px; top: {panel.position.y}px; width: {panel.size.width}px; height: {panel.size.height}px; z-index: {panel.zIndex}"
	onkeydown={handleKeydown}
>
	<!-- Toolbar (draggable) -->
	<div
		class="toolbar"
		onpointerdown={handlePointerDown}
		onpointermove={handlePointerMove}
		onpointerup={handlePointerUp}
	>
		<div class="toolbar-left">
			<button class="tb-btn" class:active={panel.mode === 'history'} onclick={toggleHistory} title="Version History">
				History
			</button>
			<button class="tb-btn" class:active={panel.mode === 'preview'} onclick={togglePreview} title="Toggle Preview">
				{panel.mode === 'preview' ? 'Edit' : 'Preview'}
			</button>
		</div>
		<div class="toolbar-right">
			{#if panel.dirty}
				<span class="dirty-dot" title="Unsaved changes"></span>
			{/if}
			<button class="tb-btn" onclick={handleSave} title="Save (Ctrl+S)">Save</button>
			<button class="tb-btn close-btn" onclick={handleClose} title="Close (Esc)">&#x2715;</button>
		</div>
	</div>

	<div class="panel-body">
		<div class="content-area">
			<!-- Title -->
			<input
				class="title-input"
				type="text"
				value={panel.doc.title}
				oninput={handleTitleInput}
				placeholder="Untitled"
			/>

			{#if panel.mode === 'edit'}
				<!-- Format toolbar -->
				<div class="format-bar">
					<button onclick={() => insertFormat('**', '**')} title="Bold"><b>B</b></button>
					<button onclick={() => insertFormat('*', '*')} title="Italic"><i>I</i></button>
					<button onclick={() => insertFormat('# ', '')} title="H1">H1</button>
					<button onclick={() => insertFormat('## ', '')} title="H2">H2</button>
					<button onclick={() => insertFormat('### ', '')} title="H3">H3</button>
					<button onclick={() => insertFormat('- ', '')} title="List">List</button>
					<button onclick={() => insertFormat('`', '`')} title="Code">Code</button>
					<button onclick={() => insertFormat('[', '](url)')} title="Link">Link</button>
				</div>

				<!-- Textarea -->
				<textarea
					bind:this={textarea}
					class="body-editor"
					value={panel.doc.body}
					oninput={handleBodyInput}
					placeholder="Start writing..."
				></textarea>
			{:else if panel.mode === 'preview'}
				<!-- Markdown Preview -->
				<div class="body-preview">
					{@html previewHtml}
				</div>
			{:else if panel.mode === 'history'}
				<!-- Version History -->
				<div class="history-list">
					{#if panel.commits.length === 0}
						<p class="empty">No commits yet</p>
					{:else}
						{#each panel.commits as commit, i}
							<button
								class="commit-row"
								class:selected={panel.selectedCommit === i}
								onclick={() => selectCommit(panel.doc.id, i)}
							>
								<span class="commit-msg">{commit.message}</span>
								<span class="commit-time">{new Date(commit.timestamp).toLocaleString()}</span>
							</button>
						{/each}
					{/if}
					{#if panel.selectedCommit !== null && panel.commits[panel.selectedCommit]}
						<div class="commit-preview">
							<h4>{panel.commits[panel.selectedCommit].snapshot_title}</h4>
							<p>{panel.commits[panel.selectedCommit].snapshot_preview}</p>
							<button
								class="restore-btn"
								onclick={() => restoreVersion(panel.doc.id, panel.commits[panel.selectedCommit!].id)}
							>
								Restore this version
							</button>
						</div>
					{/if}
				</div>
			{/if}
		</div>

		<!-- Skills Sidebar -->
		{#if skills.length > 0}
			<div class="skills-sidebar">
				<div class="sidebar-label">Skills</div>
				{#each quickActions as action}
					<button class="skill-btn" onclick={() => runSkill(action.skill, action.id)}>
						{action.label}
					</button>
				{/each}
				{#if overflowActions.length > 0}
					<button class="skill-btn overflow-toggle" onclick={() => toggleSkillsOverflow(panel.doc.id)}>
						...
					</button>
					{#if panel.skillsOverflowOpen}
						<div class="overflow-menu">
							{#each overflowActions as action}
								<button class="skill-btn" onclick={() => runSkill(action.skill, action.id)}>
									{action.label}
								</button>
							{/each}
						</div>
					{/if}
				{/if}
			</div>
		{/if}
	</div>
</div>

<style>
	.doc-panel {
		position: absolute;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 10px;
		display: flex;
		flex-direction: column;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
		overflow: hidden;
	}

	.toolbar {
		height: 44px;
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 10px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border);
		cursor: grab;
		user-select: none;
		flex-shrink: 0;
	}
	.toolbar:active {
		cursor: grabbing;
	}

	.toolbar-left,
	.toolbar-right {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.tb-btn {
		background: none;
		border: 1px solid transparent;
		color: var(--text-secondary);
		font-size: 0.75rem;
		padding: 4px 10px;
		border-radius: 4px;
		cursor: pointer;
	}
	.tb-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	.tb-btn.active {
		border-color: var(--accent);
		color: var(--accent);
	}
	.close-btn {
		font-size: 0.85rem;
		color: var(--text-muted);
	}
	.close-btn:hover {
		color: var(--error);
	}

	.dirty-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--accent);
	}

	.panel-body {
		flex: 1;
		display: flex;
		overflow: hidden;
	}

	.content-area {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.title-input {
		width: 100%;
		padding: 10px 14px;
		background: transparent;
		border: none;
		border-bottom: 1px solid var(--border);
		color: var(--text-primary);
		font-size: 1.1rem;
		font-weight: 600;
		outline: none;
		box-sizing: border-box;
	}
	.title-input::placeholder {
		color: var(--text-muted);
	}

	.format-bar {
		display: flex;
		gap: 2px;
		padding: 4px 10px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}
	.format-bar button {
		background: none;
		border: none;
		color: var(--text-secondary);
		font-size: 0.75rem;
		padding: 3px 8px;
		border-radius: 3px;
		cursor: pointer;
	}
	.format-bar button:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.body-editor {
		flex: 1;
		width: 100%;
		padding: 12px 14px;
		background: transparent;
		border: none;
		color: var(--text-primary);
		font-size: 0.9rem;
		font-family: 'SF Mono', 'Fira Code', monospace;
		line-height: 1.6;
		resize: none;
		outline: none;
		box-sizing: border-box;
	}

	.body-preview {
		flex: 1;
		padding: 12px 14px;
		overflow-y: auto;
		color: var(--text-primary);
		font-size: 0.9rem;
		line-height: 1.6;
	}
	.body-preview :global(h1) { font-size: 1.5rem; margin: 0.5em 0; }
	.body-preview :global(h2) { font-size: 1.25rem; margin: 0.5em 0; }
	.body-preview :global(h3) { font-size: 1.1rem; margin: 0.5em 0; }
	.body-preview :global(code) {
		background: var(--bg-hover);
		padding: 1px 4px;
		border-radius: 3px;
		font-size: 0.85em;
	}
	.body-preview :global(pre) {
		background: var(--bg-hover);
		padding: 10px;
		border-radius: 6px;
		overflow-x: auto;
	}
	.body-preview :global(a) {
		color: var(--accent);
	}

	/* History */
	.history-list {
		flex: 1;
		overflow-y: auto;
		padding: 8px;
	}
	.empty {
		color: var(--text-muted);
		font-size: 0.85rem;
		text-align: center;
		margin-top: 2rem;
	}
	.commit-row {
		display: flex;
		flex-direction: column;
		width: 100%;
		text-align: left;
		padding: 8px 10px;
		background: none;
		border: 1px solid transparent;
		border-radius: 6px;
		cursor: pointer;
		color: var(--text-primary);
	}
	.commit-row:hover {
		background: var(--bg-hover);
	}
	.commit-row.selected {
		border-color: var(--accent);
		background: var(--bg-hover);
	}
	.commit-msg {
		font-size: 0.85rem;
	}
	.commit-time {
		font-size: 0.7rem;
		color: var(--text-muted);
		margin-top: 2px;
	}
	.commit-preview {
		margin-top: 12px;
		padding: 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg-secondary);
	}
	.commit-preview h4 {
		margin: 0 0 6px;
		font-size: 0.9rem;
		color: var(--text-primary);
	}
	.commit-preview p {
		margin: 0 0 10px;
		font-size: 0.8rem;
		color: var(--text-secondary);
		white-space: pre-wrap;
	}
	.restore-btn {
		background: var(--accent);
		color: #000;
		border: none;
		border-radius: 4px;
		padding: 6px 14px;
		font-size: 0.8rem;
		font-weight: 600;
		cursor: pointer;
	}
	.restore-btn:hover {
		filter: brightness(1.15);
	}

	/* Skills sidebar */
	.skills-sidebar {
		width: 120px;
		border-left: 1px solid var(--border);
		padding: 10px 8px;
		display: flex;
		flex-direction: column;
		gap: 4px;
		overflow-y: auto;
		flex-shrink: 0;
	}
	.sidebar-label {
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--text-muted);
		margin-bottom: 4px;
	}
	.skill-btn {
		background: none;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-secondary);
		font-size: 0.7rem;
		padding: 5px 8px;
		cursor: pointer;
		text-align: left;
	}
	.skill-btn:hover {
		border-color: var(--accent);
		color: var(--accent);
	}
	.overflow-toggle {
		text-align: center;
		font-weight: bold;
	}
	.overflow-menu {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
</style>
