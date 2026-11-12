<script lang="ts">
	import { page } from '$app/state';
	import { currentUser, type SignedInUser } from '$lib/auth';

	/**
	 * Header account control. Re-checks on every navigation, so signing in or
	 * out is reflected immediately rather than after a reload.
	 */
	let user = $state<SignedInUser | null>(null);

	$effect(() => {
		// Depending on the pathname is what makes this re-run on navigation.
		const path = page.url.pathname;
		void path;

		let cancelled = false;
		currentUser().then((signedIn) => {
			if (!cancelled) user = signedIn;
		});
		return () => {
			cancelled = true;
		};
	});
</script>

{#if user}
	<a
		href="/me"
		class="label flex items-center gap-2 whitespace-nowrap border border-ember/50 bg-ember/10 px-2.5 py-1.5 text-ember transition-colors hover:bg-ember/20"
		title="Your player page"
	>
		<span class="size-1 rounded-full bg-ember"></span>
		{user.username}
	</a>
{:else}
	<a
		href="/login"
		class="label whitespace-nowrap border border-line-hi px-2.5 py-1.5 text-dim transition-colors hover:border-ember/60 hover:text-ink"
	>
		Sign in
	</a>
{/if}
