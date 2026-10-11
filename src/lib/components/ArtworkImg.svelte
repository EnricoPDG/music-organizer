<script lang="ts">
	import Icon from './Icon.svelte';

	let {
		src,
		alt,
		size = 48,
		rounded = 'var(--radius-art)'
	}: { src: string | null | undefined; alt: string; size?: number; rounded?: string } = $props();
	let loaded = $state(false);
	let failed = $state(false);
	$effect(() => {
		// Reset when the image changes (virtualized rows reuse components).
		void src;
		loaded = false;
		failed = false;
	});
</script>

<div
	class="relative shrink-0 overflow-hidden bg-raised text-muted"
	style:width="{size}px"
	style:height="{size}px"
	style:border-radius={rounded}
>
	{#if src && !failed}
		<img
			{src}
			{alt}
			loading="lazy"
			decoding="async"
			class="size-full object-cover transition-opacity duration-200"
			class:opacity-0={!loaded}
			onload={() => (loaded = true)}
			onerror={() => (failed = true)}
		/>
	{/if}
	{#if !src || failed || !loaded}
		<div class="absolute inset-0 grid place-items-center" aria-hidden="true">
			<Icon name="note" size={Math.max(14, size * 0.35)} />
		</div>
	{/if}
</div>
