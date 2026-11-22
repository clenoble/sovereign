<script lang="ts">
	/** Item 9 (spec ch. 03, FC1 and FC2): two or more live threads carry the
	 *  name sought and no choice holds. Each thread is one button, with its
	 *  number of documents and its creation date in local time, in History's
	 *  format; the window says the choice will be remembered. */
	import type { ThreadOffered } from '$lib/api/commands';

	let {
		name,
		threads,
		onpick,
		oncancel
	}: {
		name: string;
		threads: ThreadOffered[];
		onpick: (id: string) => void;
		oncancel: () => void;
	} = $props();

	function local(iso: string): string {
		const d = new Date(iso);
		if (isNaN(d.getTime())) return iso;
		const p = (n: number) => String(n).padStart(2, '0');
		return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
	}

	function count(n: number): string {
		return n === 1 ? '1 document' : `${n} documents`;
	}
</script>

<div class="thread-chooser" role="group" aria-label="Which thread named '{name}'?">
	<p class="question">Two or more threads are named '{name}'. Which one should be used?</p>
	<ul>
		{#each threads as t (t.id)}
			<li>
				<button class="choice" onclick={() => onpick(t.id)}>
					<span class="name">{t.name}</span>
					<span class="meta">created {local(t.created_at)} · {count(t.documents)}</span>
				</button>
			</li>
		{/each}
	</ul>
	<p class="note">Your choice will be remembered for '{name}'.</p>
	<button class="cancel" onclick={oncancel}>Cancel</button>
</div>

<style>
	.thread-chooser {
		padding: 8px 12px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--text-primary);
		font-size: 0.8rem;
	}
	.question,
	.note {
		margin: 4px 0;
	}
	.note {
		color: var(--text-muted);
	}
	ul {
		list-style: none;
		margin: 4px 0;
		padding: 0;
	}
	.choice {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		width: 100%;
		margin: 2px 0;
		padding: 6px 10px;
		background: none;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-secondary);
		cursor: pointer;
		text-align: left;
	}
	.choice:hover,
	.choice:focus-visible {
		background: var(--bg-secondary);
	}
	.meta {
		color: var(--text-muted);
	}
	.cancel {
		margin-top: 4px;
		background: none;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-secondary);
		padding: 4px 10px;
		cursor: pointer;
	}
</style>
