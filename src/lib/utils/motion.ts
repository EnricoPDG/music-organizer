/** Duration for Svelte transitions, or 0 when reduced motion is on (OS or app setting). */
export function ms(duration: number): number {
	if (typeof window === 'undefined') return duration;
	const v = getComputedStyle(document.documentElement).getPropertyValue('--motion').trim();
	return v === '0' ? 0 : duration;
}
