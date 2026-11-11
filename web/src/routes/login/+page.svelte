<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import { goto } from '$app/navigation';
	import { ApiError, currentUser, signIn } from '$lib/auth';

	let username = $state('');
	let password = $state('');
	let busy = $state(false);
	let error = $state('');

	// Already signed in? Nothing to do here.
	$effect(() => {
		currentUser().then((user) => {
			if (user) goto('/me', { replaceState: true });
		});
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (busy) return;

		busy = true;
		error = '';
		try {
			await signIn(username.trim(), password);
			// Clear the password from the DOM as soon as it is no longer needed.
			password = '';
			await goto('/me');
		} catch (err) {
			error =
				err instanceof ApiError && err.status === 401
					? 'That account name and password do not match.'
					: err instanceof ApiError
						? err.message
						: 'Could not reach the server. Try again.';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<Seo
		title="Sign in | The Commonwealth"
		description="Sign in with your in-game account to see your own stats, matches and settings."
		noindex
	/>
</svelte:head>

<div class="mx-auto flex w-full max-w-md flex-col gap-6 py-6">
	<header class="flex flex-col gap-2">
		<p class="label text-ember">Player area</p>
		<h1 class="heading text-3xl tracking-[0.06em]">Sign in</h1>
		<p class="text-sm leading-relaxed text-dim">
			Use the same account name and password you use in-game. Nothing is stored on this
			website — your password never leaves your browser.
		</p>
	</header>

	<form onsubmit={submit} class="panel-ticks flex flex-col gap-4 border border-line/80 bg-panel/40 p-6">
		<label class="flex flex-col gap-2">
			<span class="label text-faint">Account name</span>
			<input
				name="username"
				autocomplete="username"
				required
				bind:value={username}
				class="border border-line-hi bg-void/70 px-3 py-2.5 text-sm text-ink outline-none transition-colors focus:border-ember/70"
				placeholder="Your in-game name"
			/>
		</label>

		<label class="flex flex-col gap-2">
			<span class="label text-faint">Password</span>
			<input
				name="password"
				type="password"
				autocomplete="current-password"
				required
				bind:value={password}
				class="border border-line-hi bg-void/70 px-3 py-2.5 text-sm text-ink outline-none transition-colors focus:border-ember/70"
				placeholder="••••••••"
			/>
		</label>

		<div class="min-h-[2.75rem]">
			{#if error}
				<p class="border border-red/40 bg-red/10 px-3 py-2 text-sm text-red">{error}</p>
			{/if}
		</div>

		<button
			type="submit"
			disabled={busy}
			class="clip-corner mt-1 bg-red px-5 py-3 transition-colors hover:bg-ember disabled:cursor-not-allowed disabled:opacity-60"
		>
			<span class="heading text-sm tracking-[0.14em] text-white">
				{busy ? 'Checking…' : 'Sign in'}
			</span>
		</button>
	</form>

	<div class="flex flex-col gap-2 text-sm leading-relaxed text-dim">
		<p>
			Accounts are created in the game, not here: the first time you log in to the server, the
			password you type becomes your password.
		</p>
		<a href="/play" class="text-ember transition-colors hover:text-amber">How to install and log in →</a>
	</div>
</div>
