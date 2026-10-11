<script lang="ts" generics="T">
	// Renders only the visible rows, so lists of 10,000+ items scroll smoothly (SC-006).
	// Uses @tanstack/virtual-core directly: a version counter bumped by the virtualizer drives
	// Svelte's reactivity.
	import {
		Virtualizer,
		elementScroll,
		observeElementOffset,
		observeElementRect
	} from '@tanstack/virtual-core';
	import { untrack, type Snippet } from 'svelte';

	let {
		items,
		rowHeight,
		key,
		row,
		class: klass = '',
		label
	}: {
		items: T[];
		rowHeight: number;
		key: (item: T) => string;
		row: Snippet<[T, number]>;
		class?: string;
		label?: string;
	} = $props();

	let scroller = $state<HTMLDivElement | null>(null);
	let version = $state(0);
	const bump = () => untrack(() => (version = version + 1));

	const virtualizer = new Virtualizer<HTMLDivElement, HTMLDivElement>({
		count: 0,
		getScrollElement: () => scroller,
		estimateSize: () => rowHeight,
		overscan: 8,
		observeElementRect,
		observeElementOffset,
		scrollToFn: elementScroll,
		onChange: bump
	});

	$effect(() => {
		if (!scroller) return;
		const cleanup = virtualizer._didMount();
		virtualizer._willUpdate();
		return cleanup;
	});

	$effect(() => {
		const list = items;
		virtualizer.setOptions({
			...virtualizer.options,
			count: list.length,
			getItemKey: (i) => key(list[i])
		});
		virtualizer._willUpdate();
		bump();
	});

	const visible = $derived.by(() => {
		void version;
		return virtualizer.getVirtualItems();
	});
	const total = $derived.by(() => {
		void version;
		return virtualizer.getTotalSize();
	});
</script>

<div bind:this={scroller} class="scroll-area {klass}" role="list" aria-label={label}>
	<div class="relative w-full" style:height="{total}px">
		{#each visible as v (v.key)}
			{#if items[v.index]}
				<div
					role="listitem"
					class="absolute top-0 left-0 w-full"
					style:height="{rowHeight}px"
					style:transform="translateY({v.start}px)"
				>
					{@render row(items[v.index], v.index)}
				</div>
			{/if}
		{/each}
	</div>
</div>
