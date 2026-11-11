<script lang="ts">
	/**
	 * An on/off control that looks like one.
	 *
	 * These started as bordered text chips, which read as status badges: no
	 * hover, no cursor, nothing to suggest a click would do anything. Now it is
	 * a switch — a track with a travelling knob, a hover state, and a focus
	 * ring — because the whole point of this page is that a player who only
	 * knows the chat commands can find and work the controls.
	 *
	 * `variant="chip"` is the two-state variant for things that are not really
	 * on/off (a DLC pack is installed or not), same affordances without the
	 * implied knob.
	 */
	let {
		checked,
		disabled = false,
		busy = false,
		onLabel = 'on',
		offLabel = 'off',
		variant = 'switch',
		label,
		onclick
	}: {
		checked: boolean;
		disabled?: boolean;
		busy?: boolean;
		onLabel?: string;
		offLabel?: string;
		variant?: 'switch' | 'chip';
		label: string;
		onclick: () => void;
	} = $props();

	const inert = $derived(disabled || busy);
</script>

<button
	type="button"
	role="switch"
	aria-checked={checked}
	aria-label={label}
	disabled={inert}
	onclick={onclick}
	class="group inline-flex shrink-0 items-center gap-2.5 border px-3 py-2 text-left transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ember {checked
		? 'border-ember/60 bg-ember/10'
		: 'border-line-hi bg-panel/60'} {inert
		? 'opacity-45'
		: 'hover:border-ember/70 hover:bg-panel-hi active:translate-y-px'}"
>
	{#if variant === 'switch'}
		<span
			class="relative h-[1.15rem] w-[2.15rem] shrink-0 rounded-full border transition-colors duration-150 {checked
				? 'border-ember/70 bg-ember/25'
				: 'border-line-hi bg-carbon'}"
			aria-hidden="true"
		>
			<span
				class="absolute top-1/2 size-[0.8rem] -translate-y-1/2 rounded-full transition-all duration-150 {checked
					? 'left-[1.15rem] bg-ember'
					: 'left-[2px] bg-faint group-hover:bg-dim'}"
			></span>
		</span>
	{:else}
		<span
			class="h-2 w-2 transition-colors duration-150 {checked
				? 'bg-ember'
				: 'bg-line-hi group-hover:bg-faint'}"
			aria-hidden="true"
		></span>
	{/if}

	<span
		class="label w-[4.75rem] text-left transition-colors duration-150 {checked
			? 'text-ember'
			: 'text-dim group-hover:text-ink'}">{checked ? onLabel : offLabel}</span
	>
	{#if busy}
		<span class="label text-faint" aria-hidden="true">…</span>
	{/if}
</button>
