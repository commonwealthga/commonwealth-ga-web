<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	/**
	 * Player settings.
	 *
	 * These are the switches that used to be chat-command only. The page says
	 * plainly what each one costs and what it needs, because both facts are
	 * surprising: the suit toggles exist for performance, and the DLC flags do
	 * nothing without the launcher actually having the files.
	 */
	import { goto } from '$app/navigation';
	import Toast from '$lib/components/Toast.svelte';
	import { reveal } from '$lib/reveal';
	import Toggle from '$lib/components/Toggle.svelte';
	import { currentUser, type SignedInUser } from '$lib/auth';
	import { fetchSettings, setDlcInstalled, setPreference, type PlayerSettings } from '$lib/settings';

	let user = $state<SignedInUser | null>(null);
	let settings = $state<PlayerSettings | null>(null);
	let phase = $state<'checking' | 'loading' | 'ready' | 'error'>('checking');
	let problem = $state('');
	// Which control is mid-flight, so only that one shows as busy.
	let busy = $state<string | null>(null);
	// One message slot, shown as a toast so saving never reflows the page.
	let toast = $state<{ text: string; tone: 'ok' | 'error' } | null>(null);

	function notify(text: string, tone: 'ok' | 'error' = 'ok') {
		// A fresh object each time, so a repeated message re-triggers the toast.
		toast = { text, tone };
	}

	$effect(() => {
		let cancelled = false;

		currentUser()
			.then(async (signedIn) => {
				if (cancelled) return;
				if (!signedIn) {
					await goto('/login', { replaceState: true });
					return;
				}
				user = signedIn;
				phase = 'loading';
				settings = await fetchSettings();
				if (cancelled) return;
				phase = 'ready';
			})
			.catch((err: unknown) => {
				if (cancelled) return;
				problem = err instanceof Error ? err.message : 'Could not load your settings.';
				phase = 'error';
			});

		return () => {
			cancelled = true;
		};
	});

	const preference = (key: string) => settings?.preferences.find((entry) => entry.key === key);

	async function toggle(key: string) {
		const current = preference(key);
		if (!current || !settings) return;
		busy = key;

		try {
			const next = current.enabled ? '0' : '1';
			const reply = await setPreference(key, next);
			settings = {
				...settings,
				preferences: settings.preferences.map((entry) =>
					entry.key === key ? { ...entry, value: next, enabled: next === '1' } : entry
				)
			};
			notify(
				reply.applied_live
					? 'Saved and applied to your game session.'
					: 'Saved. It takes effect the next time you log in.'
			);
		} catch (err: unknown) {
			notify(err instanceof Error ? err.message : 'Could not save that.', 'error');
		} finally {
			busy = null;
		}
	}

	async function toggleDlc(identifier: string, installed: boolean) {
		if (!settings) return;
		busy = identifier;

		try {
			await setDlcInstalled(identifier, installed);
			settings = {
				...settings,
				dlc: settings.dlc.map((pack) =>
					pack.identifier === identifier ? { ...pack, installed } : pack
				)
			};
			notify(installed ? 'Pack enabled.' : 'Pack disabled.');
		} catch (err: unknown) {
			notify(err instanceof Error ? err.message : 'Could not save that.', 'error');
		} finally {
			busy = null;
		}
	}
</script>

<svelte:head>
	<Seo
		title="Settings | The Commonwealth"
		description="Your in-game preferences: solo mode, suit visibility and DLC packs, applied to your account."
		noindex
	/>
</svelte:head>

<p class="label">
	<a href="/me" class="text-faint transition-colors hover:text-ember">← Player area</a>
</p>

<header class="mt-4 mb-8">
	<h1 class="heading mt-2 text-[clamp(1.9rem,4.5vw,2.9rem)]">Settings</h1>
	<p class="mt-3 max-w-2xl text-sm leading-relaxed text-dim">
		The same switches as the in-game chat commands, without having to remember or type them.
		Changes are sent to the game server, so they apply to your account everywhere.
	</p>
</header>

{#if phase === 'error'}
	<p class="border border-red/40 bg-red/10 px-4 py-3 text-sm text-red">{problem}</p>
{:else if phase === 'checking' || phase === 'loading'}
	<div class="grid gap-4 lg:grid-cols-2">
		{#each Array(3) as _, i (i)}
			<div class="h-40 animate-pulse border border-line/60 bg-panel/30"></div>
		{/each}
	</div>
{:else if settings && user}
	{#if !settings.writable}
		<p class="mb-6 border border-line-hi bg-panel/40 px-4 py-3 text-sm leading-relaxed text-dim">
			<span class="text-ink">Read-only for now.</span>
			This site can show your current settings but cannot change them yet — the game server's
			settings endpoint is not configured. Everything below still works from in-game chat.
		</p>
	{/if}

	<Toast message={toast?.text ?? null} tone={toast?.tone ?? 'ok'} />

	<div class="grid gap-4 lg:grid-cols-2">
		<!-- Performance -->
		<section use:reveal class="panel-ticks border border-line/90 bg-panel/40 p-5">
			<h2 class="heading text-lg tracking-[0.06em] text-ink">Performance & visuals</h2>
			<p class="mt-2 text-sm leading-relaxed text-dim">
				Cosmetic suits are heavy to draw, and some of them cause stutter on older machines.
				Hiding them only changes what <span class="text-ink">you</span> see — nobody else's
				character is affected.
			</p>

			<div class="mt-4 space-y-3">
				<div class="flex items-start justify-between gap-4 border-t border-line/60 pt-3">
					<div>
						<p class="text-sm text-ink">Show original cosmetic suits</p>
						<p class="mt-1 text-xs leading-relaxed text-faint">
							Turn this off to swap stutter-prone suits for lighter ones. Improves frame
							pacing in crowded fights. <span class="text-dim">(-togglebrokensuits)</span>
						</p>
					</div>
					<Toggle
						checked={preference('show_broken_suits')?.enabled ?? false}
						busy={busy === 'show_broken_suits'}
						disabled={!settings.writable}
						label="Show original cosmetic suits"
						onclick={() => toggle('show_broken_suits')}
					/>
				</div>

				<div class="flex items-start justify-between gap-4 border-t border-line/60 pt-3">
					<div>
						<p class="text-sm text-ink">Show all suits, helmets and flairs</p>
						<p class="mt-1 text-xs leading-relaxed text-faint">
							Off hides every cosmetic on other players — the big hammer for stutters, and
							the most effective one. <span class="text-dim">(-toggleallsuits)</span>
						</p>
					</div>
					<Toggle
						checked={preference('show_all_suits')?.enabled ?? false}
						busy={busy === 'show_all_suits'}
						disabled={!settings.writable}
						label="Show all suits, helmets and flairs"
						onclick={() => toggle('show_all_suits')}
					/>
				</div>
			</div>
		</section>

		<!-- Matchmaking -->
		<section use:reveal={{ delay: 90 }} class="panel-ticks border border-line/90 bg-panel/40 p-5">
			<h2 class="heading text-lg tracking-[0.06em] text-ink">Matchmaking</h2>
			<p class="mt-2 text-sm leading-relaxed text-dim">
				How the server treats the missions you queue into.
			</p>

			<div class="mt-4 flex items-start justify-between gap-4 border-t border-line/60 pt-3">
				<div>
					<p class="text-sm text-ink">Solo mode</p>
					<p class="mt-1 text-xs leading-relaxed text-faint">
						Missions you enter spawn locked to you, so nobody else is routed in. Invites still
						work. <span class="text-dim">(-togglesolomode)</span>
					</p>
				</div>
				<Toggle
						checked={preference('solo_mode')?.enabled ?? false}
						busy={busy === 'solo_mode'}
						disabled={!settings.writable}
						label="Solo mode"
						onclick={() => toggle('solo_mode')}
					/>
			</div>
		</section>
	</div>

	<!-- DLC -->
	<section use:reveal={{ delay: 160 }} class="panel-ticks mt-4 border border-line/90 bg-panel/40 p-5">
		<h2 class="heading text-lg tracking-[0.06em] text-ink">DLC map packs</h2>
		<p class="mt-2 max-w-3xl text-sm leading-relaxed text-dim">
			These flags tell the server which map packs your install has, and keep their maps out of
			the queues while disabled. They do <span class="text-ink">not</span> download anything:
			the files come from the launcher, so install the pack first and enable it here afterwards.
			Enabling a pack you do not have will simply mean those maps fail to load for you.
			<span class="text-faint">(Launcher → <a
					class="text-ember hover:text-amber"
					href="https://github.com/Zotikus1001/commonwealth-ga-launcher/releases/latest"
					target="_blank"
					rel="noopener noreferrer">download</a
				>)</span
		>
		</p>

		<div class="mt-4">
			{#each settings.dlc as pack (pack.identifier)}
				<div class="flex items-center justify-between gap-4 border-t border-line/60 py-3">
					<div class="flex flex-wrap items-center gap-x-2.5 gap-y-1">
						<p class="text-sm text-ink">{pack.name}</p>
						{#if pack.mode}
							<span
								class="label border px-1.5 py-0.5 {pack.mode === 'pvp'
									? 'border-ember/50 text-ember'
									: 'border-steel/40 text-steel'}"
							>
								{pack.mode === 'pvp' ? 'PvP' : 'PvE'}
							</span>
						{/if}
					</div>
					<Toggle
						checked={pack.installed}
						busy={busy === pack.identifier}
						disabled={!settings.writable}
						label={pack.name}
						onLabel="enabled"
						offLabel="disabled"
						onclick={() => toggleDlc(pack.identifier, !pack.installed)}
					/>
				</div>
			{:else}
				<p class="border-t border-line/60 py-3 text-sm text-faint">No packs published yet.</p>
			{/each}
		</div>
	</section>

	<p class="mt-6 text-xs leading-relaxed text-faint">
		Every change is recorded against your account and applies from your next login. If you are in
		game, the suit and matchmaking switches also apply immediately.
	</p>
{/if}
