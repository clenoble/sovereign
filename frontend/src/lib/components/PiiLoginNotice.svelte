<script lang="ts">
	/** C-SWEEP SW6 on the phone (spec ch. 09, lot LS(a)): the notice at login,
	 *  from `piiLoginNotice` (`$lib/stores/piiReview.svelte`), in a `status`
	 *  region (the Fable maille, by delegation of Céline, 2026-11-17), with an
	 *  « OK » button that closes it. Nothing is shown when there is no notice.
	 *
	 *  Signature by the test author of lot LS(a); the body is the implementer's. */
	import { piiLoginNotice, dismissPiiLoginNotice } from '$lib/stores/piiReview.svelte';
</script>

{#if piiLoginNotice.title}
	<div class="notice" role="status">
		<span><strong>{piiLoginNotice.title}</strong><br />{piiLoginNotice.body}</span>
		<button class="ok" onclick={dismissPiiLoginNotice}>OK</button>
	</div>
{/if}

<style>
	.notice {
		position: fixed;
		left: 50%;
		bottom: 84px;
		transform: translateX(-50%);
		z-index: 340;
		display: flex;
		align-items: center;
		gap: 14px;
		max-width: min(560px, 92vw);
		padding: 10px 14px;
		background: var(--bg-panel);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
		color: var(--text-primary);
		font-size: 0.9rem;
	}
	.ok {
		background: var(--accent, #f97316);
		color: #fff;
		border: none;
		border-radius: 6px;
		padding: 6px 12px;
		cursor: pointer;
		font-weight: 600;
		flex-shrink: 0;
	}
</style>
