<script lang="ts">
	// The groove: plum → pink, the one brand device. `value` is 0–1, or null for indeterminate.
	let { value, label = 'Progress' }: { value: number | null; label?: string } = $props();
	const pct = $derived(value === null ? null : Math.round(Math.min(1, Math.max(0, value)) * 100));
</script>

<div
	class="relative h-1 w-full overflow-hidden rounded-full bg-raised"
	role="progressbar"
	aria-label={label}
	aria-valuemin={0}
	aria-valuemax={100}
	aria-valuenow={pct ?? undefined}
>
	{#if pct === null}
		<div class="groove indeterminate absolute inset-y-0 w-1/3 rounded-full"></div>
	{:else}
		<div
			class="groove absolute inset-y-0 left-0 w-full origin-left rounded-full transition-transform duration-300 ease-[var(--ease-out)]"
			style:transform="scaleX({pct / 100})"
		></div>
	{/if}
</div>

<style>
	.indeterminate {
		animation: slide calc(1.4s / max(var(--motion), 0.001)) var(--ease-out) infinite;
	}
	@keyframes slide {
		from {
			transform: translateX(-100%);
		}
		to {
			transform: translateX(300%);
		}
	}
</style>
