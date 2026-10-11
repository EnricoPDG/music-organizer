<script lang="ts">
	import { openUrl } from '@tauri-apps/plugin-opener';
	import { attempt, commands, type AccountStatus } from '#lib/api/index.ts';
	import Badge from '#lib/components/Badge.svelte';
	import Button from '#lib/components/Button.svelte';
	import Dialog from '#lib/components/Dialog.svelte';
	import { toast } from '#lib/stores/toasts.svelte.ts';

	const REDIRECT = 'http://127.0.0.1:43117/callback';

	type Guide = {
		needsSecret: boolean;
		requirement: string;
		steps: { text: string; link?: string }[];
	};
	const guides: Record<string, Guide> = {
		spotify: {
			needsSecret: false,
			requirement: 'Spotify only lets Premium accounts create developer apps.',
			steps: [
				{
					text: 'Open the Spotify developer dashboard and create an app.',
					link: 'https://developer.spotify.com/dashboard'
				},
				{ text: `Add this redirect URI to the app: ${REDIRECT}` },
				{ text: 'Tick "Web API", save, then copy the Client ID.' }
			]
		},
		youtube: {
			needsSecret: true,
			requirement:
				'Uses a free Google Cloud project. While the app is in testing, Google asks you to sign in again every 7 days.',
			steps: [
				{
					text: 'Create a project in Google Cloud and enable the YouTube Data API v3.',
					link: 'https://console.cloud.google.com/apis/library/youtube.googleapis.com'
				},
				{ text: 'Set up the OAuth consent screen as External and add yourself as a test user.' },
				{
					text: 'Create an OAuth client ID of type "Desktop app", then copy its Client ID and Client secret.',
					link: 'https://console.cloud.google.com/apis/credentials'
				}
			]
		},
		soundcloud: {
			needsSecret: true,
			requirement: 'SoundCloud only lets Artist Pro accounts register apps.',
			steps: [
				{
					text: 'Register an app in your SoundCloud developer settings.',
					link: 'https://soundcloud.com/you/apps'
				},
				{ text: `Set the redirect URI to: ${REDIRECT}` },
				{ text: 'Copy the Client ID and Client secret.' }
			]
		}
	};

	let accounts = $state<AccountStatus[]>([]);
	let open = $state(false);
	let current = $state<AccountStatus | null>(null);
	let clientId = $state('');
	let clientSecret = $state('');
	let connecting = $state(false);

	async function load() {
		accounts = (await attempt(commands.listAccounts())) ?? [];
	}
	$effect(() => {
		load();
	});

	function startConnect(a: AccountStatus) {
		current = a;
		clientId = a.clientId ?? '';
		clientSecret = '';
		open = true;
	}

	async function connect() {
		if (!current) return;
		connecting = true;
		const done = await attempt(
			commands.connectAccount(current.platform, clientId, clientSecret || null)
		);
		connecting = false;
		if (done) {
			open = false;
			toast(`${done.platformName} connected.`, 'good');
			await load();
		}
	}

	async function disconnect(a: AccountStatus) {
		// The command returns null on success; attempt() returns undefined when it failed.
		const result = await attempt(commands.disconnectAccount(a.platform));
		if (result === undefined) return;
		toast(`${a.platformName} disconnected. Its sign-in was removed from this computer.`);
		await load();
	}

	const guide = $derived(current ? guides[current.platform] : null);
</script>

<p class="max-w-prose text-muted">
	Public links work without signing in. Connect an account to import your private playlists and
	liked songs. Each platform needs a free developer app of your own; the steps below walk you
	through it. Sign-ins are kept in your system's secure credential store.
</p>

<ul class="mt-6 divide-y divide-line border-y border-line">
	{#each accounts as a (a.platform)}
		<li class="flex items-center gap-4 py-4">
			<div class="flex-1">
				<p class="font-medium">{a.platformName}</p>
				<p class="text-sm text-muted">
					{#if a.status === 'connected'}Connected{:else if a.status === 'expired'}Sign-in expired{:else}Not
						connected{/if}
				</p>
			</div>
			{#if a.status === 'connected'}
				<Badge tone="good">Connected</Badge>
				<Button size="sm" variant="ghost" onclick={() => disconnect(a)}>Disconnect</Button>
			{:else if a.status === 'expired'}
				<Badge tone="medium">Expired</Badge>
				<Button size="sm" onclick={() => startConnect(a)}>Reconnect</Button>
				<Button size="sm" variant="ghost" onclick={() => disconnect(a)}>Disconnect</Button>
			{:else}
				<Button size="sm" onclick={() => startConnect(a)}>Connect</Button>
			{/if}
		</li>
	{/each}
</ul>

{#if current && guide}
	<Dialog
		bind:open
		title="Connect {current.platformName}"
		description={guide.requirement}
		width={560}
	>
		<ol class="flex flex-col gap-3">
			{#each guide.steps as step, i (i)}
				<li class="flex gap-3">
					<span
						class="grid size-6 shrink-0 place-items-center rounded-full bg-raised text-sm font-medium"
						>{i + 1}</span
					>
					<div class="selectable">
						{step.text}
						{#if step.link}
							<button class="ml-1 text-plum hover:underline" onclick={() => openUrl(step.link!)}
								>Open</button
							>
						{/if}
					</div>
				</li>
			{/each}
		</ol>
		<form
			id="connect-form"
			class="mt-5 flex flex-col gap-3"
			onsubmit={(e) => {
				e.preventDefault();
				connect();
			}}
		>
			<label class="flex flex-col gap-1 text-sm font-medium">
				Client ID
				<input
					bind:value={clientId}
					required
					class="rounded-[var(--radius-control)] border border-line bg-surface px-3 py-2 font-normal"
					autocomplete="off"
					spellcheck="false"
				/>
			</label>
			{#if guide.needsSecret}
				<label class="flex flex-col gap-1 text-sm font-medium">
					Client secret
					<input
						bind:value={clientSecret}
						type="password"
						required
						class="rounded-[var(--radius-control)] border border-line bg-surface px-3 py-2 font-normal"
						autocomplete="off"
					/>
				</label>
			{/if}
		</form>
		{#if connecting}
			<p class="mt-4 text-sm text-muted">
				Finish signing in in your browser. This window updates when you're done.
			</p>
		{/if}
		{#snippet footer()}
			<Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
			<Button variant="primary" type="submit" form="connect-form" busy={connecting}
				>Sign in with {current?.platformName}</Button
			>
		{/snippet}
	</Dialog>
{/if}
