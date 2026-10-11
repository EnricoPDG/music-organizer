<script lang="ts">
	import '../app.css';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { fade } from 'svelte/transition';
	import { attempt, commands } from '#lib/api/index.ts';
	import Icon, { type IconName } from '#lib/components/Icon.svelte';
	import Toaster from '#lib/components/Toaster.svelte';
	import { applyAppearance, loadSettings } from '#lib/stores/settings.svelte.ts';
	import { ms } from '#lib/utils/motion.ts';

	let { children } = $props();
	let ready = $state(false);

	type Route = '/import' | '/queue' | '/review' | '/library' | '/organize';
	const nav: { href: Route; label: string; icon: IconName }[] = [
		{ href: '/import', label: 'Import', icon: 'import' },
		{ href: '/queue', label: 'Downloads', icon: 'queue' },
		{ href: '/review', label: 'Matches', icon: 'review' },
		{ href: '/library', label: 'Library', icon: 'library' },
		{ href: '/organize', label: 'Organize', icon: 'organize' }
	];

	$effect(() => {
		applyAppearance(null);
		(async () => {
			await loadSettings();
			const folders = await attempt(commands.listWatchedFolders());
			const hasTarget =
				folders?.some((f: { isDownloadTarget: boolean }) => f.isDownloadTarget) ?? false;
			if (!hasTarget && page.url.pathname !== '/onboarding') await goto(resolve('/onboarding'));
			else if (page.url.pathname === '/') await goto(resolve('/import'));
			ready = true;
		})();
	});

	const onboarding = $derived(page.url.pathname.startsWith('/onboarding'));
	const active = (href: string) => page.url.pathname.startsWith(href);
</script>

<div class="flex h-screen">
	{#if !onboarding}
		<nav
			class="flex w-52 shrink-0 flex-col gap-0.5 border-r border-line bg-surface px-3 pt-5 pb-3"
			aria-label="Main"
		>
			<div class="mb-5 flex items-center gap-2.5 px-2">
				<span
					class="groove grid size-7 place-items-center rounded-[var(--radius-control)] text-white"
				>
					<Icon name="note" size={16} />
				</span>
				<span class="display text-lg">Music Organizer</span>
			</div>
			{#each nav as item (item.href)}
				<a
					href={resolve(item.href)}
					aria-current={active(item.href) ? 'page' : undefined}
					class="relative flex h-9 items-center gap-3 rounded-[var(--radius-control)] px-2.5 text-muted transition-colors hover:bg-raised hover:text-ink aria-[current=page]:bg-raised aria-[current=page]:font-medium aria-[current=page]:text-ink"
				>
					{#if active(item.href)}
						<span class="groove absolute top-2 bottom-2 -left-3 w-[3px] rounded-r-full"></span>
					{/if}
					<Icon name={item.icon} />
					{item.label}
				</a>
			{/each}
			<a
				href={resolve('/settings')}
				aria-current={active('/settings') ? 'page' : undefined}
				class="mt-auto flex h-9 items-center gap-3 rounded-[var(--radius-control)] px-2.5 text-muted transition-colors hover:bg-raised hover:text-ink aria-[current=page]:bg-raised aria-[current=page]:text-ink"
			>
				<Icon name="settings" />
				Settings
			</a>
		</nav>
	{/if}

	<main class="scroll-area relative flex-1">
		{#if ready || onboarding}
			{#key page.url.pathname.split('/')[1]}
				<div in:fade={{ duration: ms(120) }} class="min-h-full">
					{@render children()}
				</div>
			{/key}
		{/if}
	</main>
</div>

<Toaster />
