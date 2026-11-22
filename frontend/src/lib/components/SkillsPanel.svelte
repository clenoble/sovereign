<script lang="ts">
	import { app } from '$lib/stores/app.svelte';
	import { canvas } from '$lib/stores/canvas.svelte';
	import { listAllSkills, executeSkill, rememberThreadChoice } from '$lib/api/commands';
	import type { SkillInfo, SkillActionInfo, ThreadOffered } from '$lib/api/commands';
	import ThreadChooser from './ThreadChooser.svelte';
	import { focusTrap } from '$lib/actions/focusTrap';
	import { panels, save, refresh } from '$lib/stores/documents.svelte';

	let skills = $state<SkillInfo[]>([]);
	let loading = $state(false);
	let error = $state('');
	/** Item 9, FC1 and FC2: a skill that answered `ask_thread` waits here,
	 *  with what to run again once she has answered. */
	let asking = $state<{
		name: string;
		threads: ThreadOffered[];
		skillName: string;
		action: SkillActionInfo;
		docId: string;
	} | null>(null);

	$effect(() => {
		if (app.skillsPanelVisible) {
			loadSkills();
		}
	});

	async function loadSkills() {
		loading = true;
		error = '';
		try {
			skills = await listAllSkills();
		} catch (e) {
			error = String(e);
		}
		loading = false;
	}

	async function runAction(skillName: string, action: SkillActionInfo) {
		const docId = canvas.selectedCardId;
		if (!docId) return;
		// The skill runs on the stored document and writes a content update
		// back itself (C-DOC adoption step 2): an open panel saves its edits
		// first and reloads after.
		const open = panels.find((p) => p.doc.id === docId);
		if (open) {
			await save(docId);
			if (open.dirty) {
				error = 'This document has edits that could not be saved; the skill was not run.';
				return;
			}
		}
		await run(skillName, action, docId);
	}

	async function run(skillName: string, action: SkillActionInfo, docId: string) {
		try {
			const result = await executeSkill(skillName, action.action_id, docId, '');
			if (result.kind === 'ask_thread' && result.structured_json) {
				const q = JSON.parse(result.structured_json) as { name: string; threads: ThreadOffered[] };
				asking = { name: q.name, threads: q.threads, skillName, action, docId };
				return;
			}
			if (result.kind === 'content_update') await refresh(docId);
		} catch (e) {
			console.error('Skill failed:', e);
			error = String(e);
			return;
		}
		app.skillsPanelVisible = false;
	}

	async function pickThread(id: string) {
		const q = asking;
		asking = null;
		if (!q) return;
		try {
			await rememberThreadChoice(q.name, id);
		} catch (e) {
			error = String(e);
			return;
		}
		await run(q.skillName, q.action, q.docId);
	}

	function close() {
		asking = null;
		app.skillsPanelVisible = false;
	}
</script>

{#if app.skillsPanelVisible}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="skills-backdrop" onclick={close}></div>
	<div
		class="skills-dropdown"
		role="dialog"
		aria-modal="true"
		aria-label="Skills"
		use:focusTrap={{
			active: app.skillsPanelVisible,
			onEscape: close
		}}
	>
		{#if asking}
			<ThreadChooser
				name={asking.name}
				threads={asking.threads}
				onpick={pickThread}
				oncancel={() => (asking = null)}
			/>
		{:else if loading}
			<div class="status">Loading...</div>
		{:else if error}
			<div class="status error">{error}</div>
		{:else if skills.length === 0}
			<div class="status">No skills</div>
		{:else}
			{#each skills as skill}
				{#each skill.actions as action}
					<button
						class="action-btn"
						onclick={() => runAction(skill.skill_name, action)}
						title="{skill.skill_name}: {action.action_id}"
						disabled={!canvas.selectedCardId}
					>
						{action.label}
					</button>
				{/each}
			{/each}
		{/if}
	</div>
{/if}

<style>
	.skills-backdrop {
		position: fixed;
		inset: 0;
		z-index: 79;
	}

	.skills-dropdown {
		position: absolute;
		bottom: 100%;
		left: 0;
		margin-bottom: 6px;
		min-width: 160px;
		max-height: 75vh;
		overflow-y: auto;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 8px;
		box-shadow: 0 -4px 16px rgba(0, 0, 0, 0.35);
		z-index: 81;
		padding: 4px 0;
	}

	.status {
		padding: 12px;
		color: var(--text-muted);
		font-size: 0.8rem;
		text-align: center;
	}

	.status.error {
		color: var(--error, #ef4444);
	}

	.action-btn {
		display: block;
		width: 100%;
		text-align: left;
		padding: 7px 14px;
		background: none;
		border: none;
		color: var(--text-secondary);
		font-size: 0.8rem;
		cursor: pointer;
		white-space: nowrap;
	}

	.action-btn:hover:not(:disabled) {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.action-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
