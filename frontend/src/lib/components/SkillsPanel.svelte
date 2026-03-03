<script lang="ts">
	import { app } from '$lib/stores/app.svelte';
	import { listAllSkills } from '$lib/api/commands';
	import type { SkillInfo } from '$lib/api/commands';

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

	function close() {
		app.skillsPanelVisible = false;
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			close();
		}
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
{#if app.skillsPanelVisible}
	<div class="skills-backdrop" onclick={close} onkeydown={handleKeydown}></div>
	<div class="skills-panel" onkeydown={handleKeydown}>
		<div class="panel-header">
			<span class="panel-title">Skills</span>
			<button class="close-btn" onclick={close}>&#x2715;</button>
		</div>

		<div class="panel-body">
			{#if loading}
				<div class="loading">Loading skills...</div>
			{:else if error}
				<p class="error">{error}</p>
			{:else if skills.length === 0}
				<div class="empty">No skills registered</div>
			{:else}
				{#each skills as skill}
					<div class="skill-group">
						<div class="skill-name">{skill.skill_name}</div>
						<div class="skill-actions">
							{#each skill.actions as action}
								<div class="action-row">
									<span class="action-label">{action.label}</span>
									<span class="action-id">{action.action_id}</span>
								</div>
							{/each}
						</div>
					</div>
				{/each}
			{/if}
		</div>
	</div>
{/if}

<style>
	.skills-backdrop {
		position: fixed;
		inset: 0;
		z-index: 69;
		background: rgba(0, 0, 0, 0.3);
	}

	.skills-panel {
		position: fixed;
		top: 0;
		right: 0;
		width: 360px;
		height: 100vh;
		background: var(--bg-panel);
		border-left: 1px solid var(--border);
		box-shadow: -4px 0 24px rgba(0, 0, 0, 0.4);
		z-index: 70;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 18px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.panel-title {
		font-size: 1rem;
		font-weight: 600;
		color: var(--text-primary);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		font-size: 0.9rem;
		padding: 2px 6px;
	}
	.close-btn:hover {
		color: var(--error);
	}

	.panel-body {
		flex: 1;
		overflow-y: auto;
		padding: 12px 18px;
	}

	.loading, .empty {
		color: var(--text-muted);
		font-size: 0.85rem;
		text-align: center;
		padding: 32px 0;
	}

	.error {
		color: var(--error);
		font-size: 0.8rem;
		margin: 0;
	}

	.skill-group {
		margin-bottom: 16px;
	}

	.skill-name {
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--text-primary);
		margin-bottom: 6px;
		text-transform: capitalize;
	}

	.skill-actions {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.action-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 10px;
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		border-radius: 6px;
		font-size: 0.8rem;
	}

	.action-label {
		color: var(--text-primary);
	}

	.action-id {
		color: var(--text-muted);
		font-size: 0.7rem;
		font-family: monospace;
	}
</style>
