<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon, { type IconName } from './Icon.svelte';

	let {
		icon,
		title,
		children,
		actions
	}: { icon: IconName; title: string; children?: Snippet; actions?: Snippet } = $props();
</script>

<div class="flex max-w-md flex-col items-start gap-3 py-16">
	<div class="grid size-11 place-items-center rounded-[var(--radius-panel)] bg-raised text-muted">
		<Icon name={icon} size={22} />
	</div>
	<h2 class="text-lg font-semibold">{title}</h2>
	{#if children}<div class="text-muted">{@render children()}</div>{/if}
	{#if actions}<div class="mt-1 flex gap-2">{@render actions()}</div>{/if}
</div>
