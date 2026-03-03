<script lang="ts">
	import { app } from '$lib/stores/app.svelte';
	import { canvas } from '$lib/stores/canvas.svelte';
	import { listAllSkills, executeSkill } from '$lib/api/commands';
	import type { SkillInfo, SkillActionInfo } from '$lib/api/commands';

	let skills = $state<SkillInfo[]>([]);
	let loading = $state(false);
	let error = $state('');

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
		try {
			await executeSkill(skillName, action.action_id, docId, '');
		} catch (e) {
			console.error('Skill failed:', e);
		}
		app.skillsPanelVisible = false;
	}

	function close() {
		app.skillsPanelVisible = false;
	}
</script>

{#if app.skillsPanelVisible}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="skills-backdrop" onclick={close}></div>
	<div class="skills-dropdown">
		{#if loading}
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
