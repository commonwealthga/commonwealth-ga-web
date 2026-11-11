/**
 * Ticks a number from one value to another over a short animation.
 *
 * Used where a figure appears on load: the movement draws the eye without the
 * cost of a spinner, and counting up from zero reads as "this was just
 * measured" rather than a static badge.
 *
 * Returns a cancel function; honours `prefers-reduced-motion` by setting the
 * final value immediately, and skips work when there is nothing to animate.
 */
export function countUp(
	from: number,
	to: number,
	set: (value: number) => void,
	duration = 700
): () => void {
	if (from === to) return () => {};
	if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
		set(to);
		return () => {};
	}

	const started = performance.now();
	let frame = requestAnimationFrame(function step(now) {
		// Clamp low as well as high: a frame's timestamp can predate the
		// performance.now() taken when scheduling, which makes `t` negative and
		// briefly paints values below the starting point (-1 before 0).
		const t = Math.min(1, Math.max(0, (now - started) / duration));
		const eased = 1 - Math.pow(1 - t, 3);
		set(Math.round(from + (to - from) * eased));
		if (t < 1) frame = requestAnimationFrame(step);
	});

	return () => cancelAnimationFrame(frame);
}
