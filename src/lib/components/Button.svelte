<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLButtonAttributes } from 'svelte/elements';

	type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';
	let {
		variant = 'secondary',
		size = 'md',
		busy = false,
		children,
		class: klass = '',
		...rest
	}: HTMLButtonAttributes & {
		variant?: Variant;
		size?: 'sm' | 'md';
		busy?: boolean;
		children: Snippet;
	} = $props();

	const variants: Record<Variant, string> = {
		primary: 'bg-ink text-paper hover:opacity-90',
		secondary: 'bg-raised text-ink hover:bg-line',
		ghost: 'text-muted hover:text-ink hover:bg-raised',
		danger: 'bg-bad text-white hover:opacity-90'
	};
</script>

<button
	{...rest}
	disabled={rest.disabled || busy}
	aria-busy={busy}
	class="inline-flex items-center justify-center gap-2 rounded-[var(--radius-control)] font-medium whitespace-nowrap transition-[opacity,background-color,color] duration-150 disabled:cursor-not-allowed disabled:opacity-50 {size ===
	'sm'
		? 'h-7 px-2.5 text-sm'
		: 'h-9 px-3.5'} {variants[variant]} {klass}"
>
	{#if busy}
		<span class="size-3.5 animate-spin rounded-full border-2 border-current border-t-transparent"
		></span>
	{/if}
	{@render children()}
</button>
