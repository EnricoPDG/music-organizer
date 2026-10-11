<script lang="ts">
	import { Dialog } from 'bits-ui';
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	let {
		open = $bindable(false),
		title,
		description,
		children,
		footer,
		width = 520
	}: {
		open?: boolean;
		title: string;
		description?: string;
		children: Snippet;
		footer?: Snippet;
		width?: number;
	} = $props();
</script>

<Dialog.Root bind:open>
	<Dialog.Portal>
		<Dialog.Overlay class="dialog-overlay fixed inset-0 z-40 bg-ink/40" />
		<Dialog.Content
			class="dialog-content fixed top-1/2 left-1/2 z-50 flex max-h-[85vh] w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col rounded-[var(--radius-panel)] border border-line bg-surface shadow-2xl"
			style="max-width: {width}px"
		>
			<header class="flex items-start justify-between gap-4 px-6 pt-5 pb-3">
				<div>
					<Dialog.Title class="text-lg font-semibold">{title}</Dialog.Title>
					{#if description}
						<Dialog.Description class="mt-1 text-sm text-muted">{description}</Dialog.Description>
					{/if}
				</div>
				<Dialog.Close
					class="-mr-2 rounded-[var(--radius-control)] p-1.5 text-muted hover:bg-raised hover:text-ink"
					aria-label="Close"
				>
					<Icon name="close" />
				</Dialog.Close>
			</header>
			<div class="scroll-area px-6 pb-5">{@render children()}</div>
			{#if footer}
				<footer class="flex justify-end gap-2 border-t border-line px-6 py-3">
					{@render footer()}
				</footer>
			{/if}
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>

<style>
	:global(.dialog-overlay[data-state='open']) {
		animation: fade-in calc(150ms * var(--motion)) ease-out;
	}
	:global(.dialog-content[data-state='open']) {
		animation: rise calc(180ms * var(--motion)) var(--ease-out);
	}
	@keyframes fade-in {
		from {
			opacity: 0;
		}
	}
	@keyframes rise {
		from {
			opacity: 0;
			transform: translate(-50%, calc(-50% + 8px)) scale(0.98);
		}
	}
</style>
