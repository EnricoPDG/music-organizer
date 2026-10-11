<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { fly } from 'svelte/transition';
	import {
		attempt,
		commands,
		errorMessage,
		isAppError,
		type AccountStatus,
		type AppError,
		type CollectionSummary,
		type LinkPreview
	} from '#lib/api/index.ts';
	import ArtworkImg from '#lib/components/ArtworkImg.svelte';
	import Badge from '#lib/components/Badge.svelte';
	import Button from '#lib/components/Button.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import ProgressBar from '#lib/components/ProgressBar.svelte';
	import { refresh as refreshQueue } from '#lib/stores/queue.svelte.ts';
	import { toast } from '#lib/stores/toasts.svelte.ts';
	import { duration, plural } from '#lib/utils/format.ts';
	import { ms } from '#lib/utils/motion.ts';

	let url = $state('');
	let resolving = $state(false);
	let importing = $state(false);
	let preview = $state<LinkPreview | null>(null);
	let error = $state<AppError | { kind: 'internal'; message: string } | null>(null);
	let accounts = $state<AccountStatus[]>([]);
	let mine = $state<Record<string, CollectionSummary[]>>({});

	const kindLabel: Record<string, string> = {
		playlist: 'Playlist',
		album: 'Album',
		track: 'Track',
		user_likes: 'Liked songs',
		artist: 'Artist'
	};

	async function resolveLink(value = url) {
		const link = value.trim();
		if (!link || resolving) return;
		url = link;
		resolving = true;
		error = null;
		preview = null;
		try {
			preview = await commands.resolveLink(link);
		} catch (e) {
			error = isAppError(e) ? e : { kind: 'internal', message: errorMessage(e) };
		} finally {
			resolving = false;
		}
	}

	function onPaste(e: ClipboardEvent) {
		const text = e.clipboardData?.getData('text') ?? '';
		if (/^\s*(https?:\/\/)?[\w.-]+\.\w+\//.test(text)) {
			e.preventDefault();
			resolveLink(text);
		}
	}

	async function startImport(autoDownload: boolean) {
		if (!preview) return;
		importing = true;
		const started = await attempt(commands.startImport(preview.url, autoDownload));
		importing = false;
		if (!started) return;
		await refreshQueue();
		const parts = [plural(started.queued, 'track') + (autoDownload ? ' downloading' : ' added')];
		if (started.skipped) parts.push(`${started.skipped} already in your library`);
		toast(parts.join(', ') + '.', 'good', {
			label: 'View downloads',
			run: () => goto(resolve('/queue'))
		});
		preview = null;
		url = '';
	}

	$effect(() => {
		(async () => {
			accounts = (await attempt(commands.listAccounts())) ?? [];
			for (const a of accounts.filter((x) => x.status === 'connected')) {
				const list = await attempt(commands.listMyCollections(a.platform));
				if (list) mine[a.platform] = list;
			}
		})();
	});

	const connected = $derived(accounts.filter((a) => a.status === 'connected'));
	const needsAccount = $derived(
		error &&
			['private', 'not_connected', 'auth_expired', 'public_source_changed'].includes(error.kind)
	);
</script>

<div class="mx-auto max-w-3xl px-10 pt-12 pb-16">
	<h1 class="display text-3xl">Import</h1>
	<p class="mt-2 text-muted">
		Paste a playlist, album or track from Spotify, YouTube or SoundCloud.
	</p>

	<form
		class="mt-8 flex items-center gap-2 rounded-[var(--radius-panel)] border border-line bg-surface p-1.5 pl-4 transition-colors focus-within:border-plum"
		onsubmit={(e) => {
			e.preventDefault();
			resolveLink();
		}}
	>
		<span class="text-muted"><Icon name="link" /></span>
		<input
			bind:value={url}
			onpaste={onPaste}
			class="h-10 flex-1 bg-transparent outline-none placeholder:text-muted/70 focus-visible:shadow-none"
			placeholder="https://open.spotify.com/playlist/…"
			aria-label="Playlist link"
			spellcheck="false"
			autocomplete="off"
		/>
		<Button variant="primary" type="submit" busy={resolving} disabled={!url.trim()}>Preview</Button>
	</form>

	{#if resolving}
		<div class="mt-6" in:fly={{ y: 6, duration: ms(160) }}>
			<ProgressBar value={null} label="Reading the playlist" />
			<p class="mt-2 text-sm text-muted">Reading the playlist…</p>
		</div>
	{/if}

	{#if error}
		<div
			class="mt-6 flex items-start gap-3 rounded-[var(--radius-panel)] border border-bad/30 bg-bad/6 px-4 py-3"
			role="alert"
			in:fly={{ y: 6, duration: ms(160) }}
		>
			<span class="mt-0.5 text-bad"><Icon name="alert" /></span>
			<div class="flex-1">
				<p class="selectable">{error.message}</p>
				{#if needsAccount}
					<a
						class="mt-2 inline-block text-sm font-medium text-plum hover:underline"
						href={resolve('/settings/accounts')}
					>
						Open account settings
					</a>
				{:else if error.kind === 'tool_missing'}
					<a
						class="mt-2 inline-block text-sm font-medium text-plum hover:underline"
						href={resolve('/settings/tools')}
					>
						Install download tools
					</a>
				{/if}
			</div>
		</div>
	{/if}

	{#if preview}
		<section class="mt-8" in:fly={{ y: 10, duration: ms(220) }} aria-label="Preview">
			<div class="flex gap-6">
				<ArtworkImg src={preview.artworkUrl} alt="" size={152} rounded="var(--radius-panel)" />
				<div class="flex min-w-0 flex-1 flex-col">
					<p class="text-sm text-muted">{kindLabel[preview.kind]} on {preview.platformName}</p>
					<h2 class="display mt-1 text-2xl break-words">{preview.name}</h2>
					<p class="mt-1 text-muted">
						{[preview.owner, plural(preview.trackCount, 'track')].filter(Boolean).join(', ')}
					</p>
					<div class="mt-3 flex flex-wrap gap-2">
						{#if preview.fetchMode === 'public_fallback'}
							<Badge tone="medium">Best-effort public import</Badge>
						{/if}
						{#if preview.unavailableCount > 0}
							<Badge tone="neutral"
								>{plural(preview.unavailableCount, 'item')} can't be imported</Badge
							>
						{/if}
					</div>
					<div class="mt-auto flex gap-2 pt-4">
						<Button
							variant="primary"
							busy={importing}
							onclick={() => startImport(true)}
							disabled={preview.trackCount === 0}
						>
							Import and download
						</Button>
						<Button
							variant="ghost"
							disabled={importing || preview.trackCount === 0}
							onclick={() => startImport(false)}
						>
							Add without downloading
						</Button>
					</div>
				</div>
			</div>

			{#if preview.fetchMode === 'public_fallback'}
				<p class="mt-5 max-w-prose text-sm text-muted">
					Spotify only shares the track list of playlists you own through its official API, so this
					one is read from Spotify's public page. It works today, but it may stop working if Spotify
					changes that page.
				</p>
			{/if}

			{#if preview.sample.length}
				<ol class="mt-6 divide-y divide-line border-y border-line">
					{#each preview.sample as t, i (i)}
						<li class="flex items-center gap-4 py-2.5">
							<span class="tabular w-6 text-right text-sm text-muted">{i + 1}</span>
							<div class="min-w-0 flex-1">
								<p class="truncate">{t.title}</p>
								<p class="truncate text-sm text-muted">{t.artists.join(', ')}</p>
							</div>
							<span class="tabular text-sm text-muted">{duration(t.durationMs)}</span>
						</li>
					{/each}
				</ol>
				{#if preview.trackCount > preview.sample.length}
					<p class="mt-2 text-sm text-muted">
						and {plural(preview.trackCount - preview.sample.length, 'more track')}
					</p>
				{/if}
			{/if}
		</section>
	{/if}

	{#if !preview && !resolving && connected.length}
		<section class="mt-12" aria-labelledby="mine">
			<h2 id="mine" class="text-lg font-semibold">From your accounts</h2>
			{#each connected as a (a.platform)}
				{#if mine[a.platform]?.length}
					<h3 class="mt-5 text-sm text-muted">{a.platformName}</h3>
					<ul class="mt-2 divide-y divide-line border-y border-line">
						{#each mine[a.platform] as c (c.id)}
							<li>
								<button
									class="flex w-full items-center gap-3 py-2.5 text-left hover:bg-raised/60"
									onclick={() => resolveLink(c.url)}
								>
									<ArtworkImg src={c.artworkUrl} alt="" size={40} />
									<span class="min-w-0 flex-1 truncate">{c.name}</span>
									{#if c.trackCount !== null}
										<span class="text-sm text-muted">{plural(c.trackCount, 'track')}</span>
									{/if}
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			{/each}
		</section>
	{/if}
</div>
