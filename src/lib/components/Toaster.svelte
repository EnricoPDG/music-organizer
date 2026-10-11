<script lang="ts">
	import { fly } from 'svelte/transition';
	import { dismiss, toasts } from '#lib/stores/toasts.svelte.ts';
	import { ms } from '#lib/utils/motion.ts';
	import Icon from './Icon.svelte';
</script>

<div
	class="pointer-events-none fixed right-4 bottom-4 z-50 flex w-96 flex-col gap-2"
	aria-live="polite"
>
	{#each toasts as t (t.id)}
		<div
			transition:fly={{ y: 12, duration: ms(180) }}
			class="pointer-events-auto flex items-start gap-3 rounded-[var(--radius-panel)] border border-line bg-surface px-4 py-3 shadow-lg"
			role={t.tone === 'bad' ? 'alert' : 'status'}
		>
			<span class="mt-0.5" class:text-good={t.tone === 'good'} class:text-bad={t.tone === 'bad'}>
				<Icon name={t.tone === 'bad' ? 'alert' : t.tone === 'good' ? 'check' : 'note'} size={16} />
			</span>
			<p class="selectable flex-1 text-sm">{t.message}</p>
			{#if t.action}
				<button
					class="text-sm font-medium text-plum hover:underline"
					onclick={() => {
						t.action?.run();
						dismiss(t.id);
					}}>{t.action.label}</button
				>
			{/if}
			<button class="text-muted hover:text-ink" aria-label="Dismiss" onclick={() => dismiss(t.id)}>
				<Icon name="close" size={14} />
			</button>
		</div>
	{/each}
</div>
