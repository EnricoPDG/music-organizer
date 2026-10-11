<script lang="ts">
	import { resolve } from '$app/paths';
	import { attempt, commands, type BatchView, type JobView } from '#lib/api/index.ts';
	import ArtworkImg from '#lib/components/ArtworkImg.svelte';
	import Badge from '#lib/components/Badge.svelte';
	import Button from '#lib/components/Button.svelte';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import ProgressBar from '#lib/components/ProgressBar.svelte';
	import VirtualList from '#lib/components/VirtualList.svelte';
	import { isFinal, jobsFor, queue, refresh, stateLabel } from '#lib/stores/queue.svelte.ts';
	import { plural } from '#lib/utils/format.ts';

	const ROW = 60;
	let expanded = $state<Record<string, boolean>>({});

	$effect(() => {
		refresh();
	});

	// The newest batch starts expanded.
	$effect(() => {
		const first = queue.batches[0];
		if (first && expanded[first.batchId] === undefined) expanded[first.batchId] = true;
	});

	function finished(b: BatchView) {
		return b.done + b.skipped + b.failed;
	}

	function batchProgress(b: BatchView): number {
		if (!b.total) return 0;
		const live = jobsFor(b.batchId)
			.filter((j) => !isFinal(j.state))
			.reduce((sum, j) => sum + (j.progress ?? 0), 0);
		return Math.min(1, (finished(b) + live) / b.total);
	}

	function sorted(jobs: JobView[]): JobView[] {
		// Problems first, then work in progress, then the rest in playlist order.
		const rank = (j: JobView) =>
			j.state === 'failed'
				? 0
				: !isFinal(j.state) && j.state !== 'queued' && j.state !== 'paused'
					? 1
					: 2;
		return [...jobs].sort((a, b) => rank(a) - rank(b));
	}

	async function act(
		kind: 'pause' | 'resume' | 'cancel' | 'retry',
		batchId: string | null,
		jobIds: string[] | null
	) {
		const fn = {
			pause: commands.pauseJobs,
			resume: commands.resumeJobs,
			cancel: commands.cancelJobs,
			retry: commands.retryJobs
		}[kind];
		await attempt(fn(jobIds, batchId));
		await refresh();
	}

	const hasActive = (b: BatchView) =>
		b.active > 0 || jobsFor(b.batchId).some((j) => j.state === 'queued');
	const hasPaused = (b: BatchView) => jobsFor(b.batchId).some((j) => j.state === 'paused');
</script>

<div class="mx-auto max-w-4xl px-10 pt-12 pb-16">
	<h1 class="display text-3xl">Downloads</h1>

	{#if queue.loaded && queue.batches.length === 0}
		<EmptyState icon="queue" title="Nothing is downloading">
			Import a playlist and its tracks will show up here while they download.
			{#snippet actions()}
				<a href={resolve('/import')}><Button variant="primary">Import a playlist</Button></a>
			{/snippet}
		</EmptyState>
	{/if}

	<div class="mt-8 flex flex-col gap-8">
		{#each queue.batches as b (b.batchId)}
			{@const jobs = jobsFor(b.batchId)}
			<section aria-labelledby="b-{b.batchId}">
				<div class="flex items-center gap-3">
					<button
						class="flex min-w-0 flex-1 items-center gap-3 text-left"
						aria-expanded={!!expanded[b.batchId]}
						onclick={() => (expanded[b.batchId] = !expanded[b.batchId])}
					>
						<span
							class="text-muted transition-transform duration-150"
							class:rotate-90={expanded[b.batchId]}
						>
							<svg
								width="14"
								height="14"
								viewBox="0 0 24 24"
								fill="none"
								stroke="currentColor"
								stroke-width="2"
								aria-hidden="true"><path d="m9 6 6 6-6 6" /></svg
							>
						</span>
						<h2 id="b-{b.batchId}" class="truncate text-lg font-semibold">{b.name}</h2>
					</button>
					<span class="tabular text-sm whitespace-nowrap text-muted"
						>{finished(b)} of {b.total}</span
					>
					{#if hasActive(b)}
						<Button size="sm" variant="ghost" onclick={() => act('pause', b.batchId, null)}
							><Icon name="pause" size={14} />Pause</Button
						>
					{:else if hasPaused(b)}
						<Button size="sm" variant="ghost" onclick={() => act('resume', b.batchId, null)}
							><Icon name="play" size={14} />Resume</Button
						>
					{/if}
					{#if b.failed > 0}
						<Button size="sm" variant="ghost" onclick={() => act('retry', b.batchId, null)}
							><Icon name="retry" size={14} />Retry failed</Button
						>
					{/if}
				</div>
				<div class="mt-3"><ProgressBar value={batchProgress(b)} label="{b.name} progress" /></div>
				<p class="mt-2 text-sm text-muted">
					{plural(b.done, 'track')} downloaded{#if b.skipped}, {b.skipped} already in your library{/if}{#if b.failed},
						<span class="text-bad">{b.failed} failed</span>{/if}
				</p>

				{#if expanded[b.batchId]}
					<VirtualList
						items={sorted(jobs)}
						rowHeight={ROW}
						key={(j) => j.jobId}
						class="mt-3 max-h-[60vh] border-y border-line"
						label="Tracks in {b.name}"
					>
						{#snippet row(j: JobView)}
							<div class="flex h-full items-center gap-3 border-b border-line/60 px-1">
								<ArtworkImg src={j.artworkUrl} alt="" size={40} />
								<div class="min-w-0 flex-1">
									<p class="truncate">{j.title}</p>
									{#if j.state === 'failed' && j.errorMessage}
										<p class="truncate text-sm text-bad" title={j.errorMessage}>{j.errorMessage}</p>
									{:else if j.state === 'paused' && j.errorMessage}
										<p class="truncate text-sm text-warn">{j.errorMessage}</p>
									{:else}
										<p class="truncate text-sm text-muted">{j.artists.join(', ')}</p>
									{/if}
								</div>
								{#if j.confidenceLevel === 'low' || j.confidenceLevel === 'medium'}
									<Badge tone={j.confidenceLevel}
										>{j.confidenceLevel === 'low' ? 'Low match' : 'Check match'}</Badge
									>
								{/if}
								<div class="flex w-40 flex-col items-end gap-1">
									<span
										class="text-sm"
										class:text-muted={j.state !== 'failed'}
										class:text-bad={j.state === 'failed'}
										class:text-good={j.state === 'done'}
									>
										{stateLabel[j.state] ?? j.state}
									</span>
									{#if j.state === 'downloading'}
										<div class="w-full">
											<ProgressBar value={j.progress} label="{j.title} download" />
										</div>
									{/if}
								</div>
								<div class="flex w-16 justify-end">
									{#if j.state === 'failed' || j.state === 'cancelled'}
										<button
											class="rounded-[var(--radius-control)] p-1.5 text-muted hover:bg-raised hover:text-ink"
											aria-label="Retry {j.title}"
											onclick={() => act('retry', null, [j.jobId])}
										>
											<Icon name="retry" size={16} />
										</button>
									{:else if !isFinal(j.state)}
										<button
											class="rounded-[var(--radius-control)] p-1.5 text-muted hover:bg-raised hover:text-ink"
											aria-label="Cancel {j.title}"
											onclick={() => act('cancel', null, [j.jobId])}
										>
											<Icon name="close" size={16} />
										</button>
									{/if}
								</div>
							</div>
						{/snippet}
					</VirtualList>
				{/if}
			</section>
		{/each}
	</div>
</div>
