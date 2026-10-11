<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';

	let { children } = $props();
	const tabs = [
		{ href: '/settings', label: 'General' },
		{ href: '/settings/accounts', label: 'Accounts' },
		{ href: '/settings/tools', label: 'Tools' }
	] as const;
</script>

<div class="mx-auto max-w-3xl px-10 pt-12 pb-16">
	<h1 class="display text-3xl">Settings</h1>
	<nav class="mt-6 flex gap-1 border-b border-line" aria-label="Settings sections">
		{#each tabs as t (t.href)}
			{@const active = page.url.pathname === t.href}
			<a
				href={resolve(t.href)}
				aria-current={active ? 'page' : undefined}
				class="relative px-3 py-2 text-muted transition-colors hover:text-ink aria-[current=page]:text-ink"
			>
				{t.label}
				{#if active}<span class="groove absolute inset-x-2 -bottom-px h-0.5 rounded-full"
					></span>{/if}
			</a>
		{/each}
	</nav>
	<div class="mt-8">{@render children()}</div>
</div>
