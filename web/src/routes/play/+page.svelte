<script lang="ts">
	import Seo from '$lib/components/Seo.svelte';
	import Button from '$lib/components/Button.svelte';
	import Panel from '$lib/components/Panel.svelte';

	// Rendered as separate tokens so the line wraps between flags instead of
	// mid-word, while a copy of the block is still one space-separated string.
	const LAUNCH_FLAGS = [
		'-host=commonwealth.ydns.eu',
		'-hostdns=commonwealth.ydns.eu',
		'-seekfreeloading',
		'-tcp=300'
	];

	const faq = [
		{
			q: 'How do I play?',
			a: 'Use the community launcher (recommended) — it downloads what is needed and applies client patches. If you prefer not to use it, you can launch the retail client directly with the flags shown above, minus the patches and the optional extra content.'
		},
		{
			q: 'Where do I get the game itself?',
			a: 'From Steam. It is unlisted but still downloadable: use steam://install/17020 or search the store for app 17020. The launcher and the manual method both work on top of that install — neither of them downloads the game for you.'
		},
		{
			q: 'How do I create an account?',
			a: 'There is no sign-up form — an account is created automatically the first time you log in. The password you type on that first login becomes your password, so pick one you will remember. Your account name is also your in-game display name and cannot contain special characters.'
		},
		{
			q: 'I forgot my password. Can you reset it?',
			a: 'Staff can clear the stored credential so the next login registers a new password. Ask in Discord — nobody can recover the old one, it is never stored in readable form.'
		},
		{
			q: 'Is it free? Are there microtransactions?',
			a: 'Free. There is nothing to buy, no cash shop and no paid tiers. The server is run by volunteers and the software is open source.'
		},
		{
			q: 'Something is not working right. Can I report a bug?',
			a: 'Please do, on our Discord. Report what you were doing, what happened, and if possible attach the client log. The more specific, the faster it gets fixed.'
		}
	];

	const issues = [
		{
			title: 'Stuck at the login screen or "cannot connect"',
			body: 'Almost always a launch flag problem. Launching the .exe directly without the -host/-hostdns flags sends you to the long-dead official servers. Use the launcher, or double-check the shortcut target exactly.'
		},
		{
			title: 'Wrong password / account already exists',
			body: 'The password is the one you typed on your very first login on this server. If you do not have it, ask staff to reset the credential in Discord.'
		},
		{
			title: 'Missing maps or content',
			body: 'Restored and extra content comes from the launcher and its optional downloads. The manual launch method only loads what is already in your game folder.'
		},
		{
			title: 'Crashes on startup',
			body: 'Make sure the client is not being run as a different user, and that your antivirus has not quarantined the patched DLL. Post the crash log in Discord if it persists.'
		}
	];
</script>

<svelte:head>
	<Seo
		title="Play Global Agenda on The Commonwealth"
		description="Download the launcher, sign in with a Global Agenda account and play on a live community server. Free, no purchase, works with the retail game files."
	/>
</svelte:head>

<header class="mb-10">
	<p class="label text-ember">Setup</p>
	<h1 class="heading mt-3 text-[clamp(2rem,5vw,3.25rem)]">Get started</h1>
	<p class="mt-4 max-w-2xl text-base leading-relaxed text-dim">
		Install Global Agenda from Steam, run it once so its prerequisites install, then either use the
		community launcher or launch the client yourself. About five minutes, plus however long Steam
		takes to download the game. It runs on Windows, and on Linux through Proton or Wine.
	</p>
</header>

<!-- Step 1: nobody can skip this. The launcher finds an existing install, it
     does not download the game for you. -->
<Panel label="Step 1 · install the game" class="mb-4" bodyClass="flex flex-col gap-4">
	<div class="grid gap-5 lg:grid-cols-[1.2fr_1fr] lg:items-center">
		<div class="flex flex-col gap-3">
			<h2 class="heading text-xl tracking-[0.08em]">Get the client from Steam</h2>
			<p class="text-sm leading-relaxed text-dim">
				Global Agenda is unlisted on Steam but still downloadable. Open the install link below
				and Steam will fetch the retail client — this is the game itself, not the server. You
				need it whichever way you play.
			</p>
			<div class="flex flex-wrap items-center gap-3">
				<Button href="steam://install/17020">Install via Steam</Button>
				<a
					href="https://store.steampowered.com/app/17020/"
					target="_blank"
					rel="noopener noreferrer"
					class="label border border-line-hi px-3 py-2 text-dim transition-colors hover:border-ember/60 hover:text-ink"
					>Store page ↗</a
				>
			</div>

			<div class="border-l-2 border-ember/60 pl-3">
				<p class="text-sm leading-relaxed text-dim">
					<span class="text-ink">Then run it once from Steam.</span> The first launch is what
					puts the prerequisites in place — DirectX, PhysX and the Visual C++ runtimes the
					client expects. It will not reach the old official servers; that is expected. Close
					it once it settles on the login screen and carry on below.
				</p>
			</div>
		</div>

		<div class="flex flex-col gap-2 border border-line/70 bg-void/40 p-4">
			<span class="label text-faint">if the link does nothing</span>
			<p class="text-sm leading-relaxed text-dim">
				Browsers sometimes refuse <code class="text-steel">steam://</code> links. Open Steam
				yourself, then run the install from your browser or the Steam console with
				<code class="text-steel">app_install 17020</code>.
			</p>
		</div>
	</div>
</Panel>

<div class="grid gap-4 lg:grid-cols-2">
	<!-- Option 1 -->
	<Panel label="Option 1 · recommended" class="flex flex-col" bodyClass="flex flex-1 flex-col gap-4">
		<h2 class="heading text-xl tracking-[0.08em]">Use the community launcher</h2>
		<p class="text-sm leading-relaxed text-dim">
			Maintained by <span class="text-ink">Sizzh</span>. It finds the installation you made in
			step 1, applies client-side patches, and launches you straight into the server.
		</p>
		<ul class="flex flex-col gap-2 text-sm text-dim">
			{#each ['Finds your existing game install, or lets you pick the folder', 'One-click client patches, with an undo for each', 'Optional PvP and PvE maps from older versions of the game, installed and repaired in place', 'Profiles for graphics and controls, plus server status'] as line (line)}
				<li class="flex gap-2.5">
					<span class="mt-1.5 size-1 shrink-0 rotate-45 bg-ember"></span>
					<span>{line}</span>
				</li>
			{/each}
		</ul>
		<div class="mt-auto pt-2">
			<Button href="https://github.com/Zotikus1001/commonwealth-ga-launcher/releases/latest">
				Get the launcher
			</Button>
			<p class="label mt-3 text-faint">Latest release, with a download per OS</p>
		</div>
	</Panel>

	<!-- Option 2 -->
	<Panel label="Option 2 · manual" class="flex flex-col" bodyClass="flex flex-1 flex-col gap-4">
		<h2 class="heading text-xl tracking-[0.08em]">Launch the retail client yourself</h2>
		<p class="text-sm leading-relaxed text-dim">
			No launcher, no patches, but still the game from step 1. Create a desktop shortcut to
			<span class="text-ink">GlobalAgenda.exe</span> inside the game's <code class="text-steel">Binaries</code> folder,
			then open its properties and append this to the <span class="text-ink">Target</span> field:
		</p>

		<pre
			class="flex flex-wrap gap-x-2 gap-y-1 border border-line/80 bg-void/70 p-3 font-mono text-xs leading-relaxed text-amber select-all"
		>{#each LAUNCH_FLAGS as flag (flag)}<span class="whitespace-nowrap">{flag}</span>{/each}</pre>

		<p class="text-sm leading-relaxed text-dim">
			That is the whole trick. You will not get the launcher's patches, and you will not be able to
			play content the launcher would have downloaded.
		</p>
	</Panel>
</div>

<section class="mt-14">
	<div class="mb-5 border-b border-line/60 pb-3">
		<h2 class="heading text-2xl tracking-[0.08em]">Your account</h2>
	</div>
	<div class="grid gap-4 sm:grid-cols-3">
		{#each [{ t: 'Created on first login', b: 'There is no sign-up page. The first successful login registers the account.' }, { t: 'First password wins', b: 'Whatever you type on that first login becomes the password for every login after it.' }, { t: 'Name is your display name', b: 'Your account name is what other players see. Letters, digits and no special characters.' }] as item (item.t)}
			<div class="border border-line/80 bg-panel/40 p-5">
				<h3 class="text-sm font-medium text-ink">{item.t}</h3>
				<p class="mt-2 text-sm leading-relaxed text-dim">{item.b}</p>
			</div>
		{/each}
	</div>
</section>

<section id="troubleshooting" class="mt-14 scroll-mt-28">
	<div class="mb-5 border-b border-line/60 pb-3">
		<h2 class="heading text-2xl tracking-[0.08em]">Troubleshooting</h2>
	</div>

	<div class="grid gap-4 md:grid-cols-2">
		{#each issues as issue (issue.title)}
			<div class="border border-line/80 bg-panel/40 p-5">
				<h3 class="text-sm font-medium text-ink">{issue.title}</h3>
				<p class="mt-2 text-sm leading-relaxed text-dim">{issue.body}</p>
			</div>
		{/each}
	</div>

	<div class="mt-4 flex flex-col gap-4 border border-line/80 bg-panel/40 p-5 sm:flex-row sm:items-center sm:justify-between">
		<p class="text-sm leading-relaxed text-dim">
			Not covered here? Report it in Discord with your client log — bug reports are genuinely
			welcome, and most fixes start as one.
		</p>
		<Button href="https://discord.gg/CqACf4qArk" variant="ember" class="shrink-0">Report a bug</Button>
	</div>
</section>

<section class="mt-14">
	<div class="mb-5 border-b border-line/60 pb-3">
		<h2 class="heading text-2xl tracking-[0.08em]">Questions</h2>
	</div>

	<div class="border border-line/80 bg-panel/30">
		{#each faq as item, i (item.q)}
			<details class="group border-b border-line/60 last:border-b-0">
				<summary
					class="flex cursor-pointer items-center gap-3 px-5 py-4 text-sm text-ink transition-colors hover:text-amber"
				>
					<span class="label text-faint">{String(i + 1).padStart(2, '0')}</span>
					<span class="font-medium">{item.q}</span>
					<span class="ml-auto text-faint transition-transform group-open:rotate-45">+</span>
				</summary>
				<p class="px-5 pb-5 pl-[3.1rem] text-sm leading-relaxed text-dim">{item.a}</p>
			</details>
		{/each}
	</div>
</section>
