/**
 * `use:reveal` — fade an element up the first time it scrolls into view.
 *
 * Deliberately tiny: no dependency, no layout thrash, and a no-op when the
 * visitor prefers reduced motion or the browser has no IntersectionObserver.
 */
export function reveal(
	node: HTMLElement,
	options: { delay?: number } = {}
): { destroy: () => void } | undefined {
	if (typeof IntersectionObserver === 'undefined') return;
	if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

	const delay = options.delay ?? 0;
	node.style.opacity = '0';

	const observer = new IntersectionObserver(
		(entries) => {
			for (const entry of entries) {
				if (!entry.isIntersecting) continue;
				node.style.animation = `rise 620ms cubic-bezier(0.22, 0.68, 0, 1) ${delay}ms both`;
				observer.unobserve(node);
			}
		},
		{ rootMargin: '0px 0px -8% 0px' }
	);

	observer.observe(node);

	return {
		destroy() {
			observer.disconnect();
		}
	};
}
