<script lang="ts">
	import type { Snippet } from 'svelte';

	type Tone = 'high' | 'medium' | 'low' | 'neutral' | 'good' | 'bad';
	let { tone = 'neutral', children }: { tone?: Tone; children: Snippet } = $props();

	// Match confidence reads as a traffic light; neutral for plain labels.
	const tones: Record<Tone, string> = {
		high: 'text-good bg-good/12',
		medium: 'text-warn bg-warn/14',
		low: 'text-bad bg-bad/12',
		good: 'text-good bg-good/12',
		bad: 'text-bad bg-bad/12',
		neutral: 'text-muted bg-raised'
	};
</script>

<span
	class="inline-flex h-5 items-center gap-1 rounded-full px-2 text-xs font-medium whitespace-nowrap {tones[
		tone
	]}"
>
	{@render children()}
</span>
