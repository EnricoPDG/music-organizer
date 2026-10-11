<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { open } from '@tauri-apps/plugin-dialog';
	import { attempt, commands } from '#lib/api/index.ts';
	import Button from '#lib/components/Button.svelte';
	import Icon from '#lib/components/Icon.svelte';

	let folder = $state<string | null>(null);
	let saving = $state(false);

	async function choose() {
		const picked = await open({ directory: true, title: 'Choose your music folder' });
		if (typeof picked === 'string') folder = picked;
	}

	async function start() {
		if (!folder) return;
		saving = true;
		const saved = await attempt(commands.addWatchedFolder(folder, true));
		saving = false;
		if (saved) await goto(resolve('/import'));
	}
</script>

<div class="mx-auto flex min-h-screen max-w-xl flex-col justify-center gap-8 px-8 py-16">
	<div class="flex flex-col gap-4">
		<span class="groove grid size-14 place-items-center rounded-[var(--radius-panel)] text-white">
			<Icon name="note" size={30} />
		</span>
		<h1 class="display text-3xl">Bring your music home.</h1>
		<p class="max-w-prose text-lg text-muted">
			Paste a playlist from Spotify, YouTube or SoundCloud and get tagged audio files on your
			computer. First, pick the folder where your music should live.
		</p>
	</div>

	<div class="flex flex-col gap-3">
		<button
			onclick={choose}
			class="flex items-center gap-3 rounded-[var(--radius-panel)] border border-dashed border-line bg-surface px-4 py-4 text-left transition-colors hover:border-plum"
		>
			<Icon name="folder" size={22} />
			<span class="flex-1 truncate">
				{#if folder}
					<span class="selectable font-medium">{folder}</span>
				{:else}
					<span class="text-muted">Choose a folder…</span>
				{/if}
			</span>
			{#if folder}<span class="text-sm text-muted">Change</span>{/if}
		</button>
		<p class="text-sm text-muted">
			Existing music in this folder stays untouched. Downloads are added in Artist / Album folders.
		</p>
	</div>

	<div>
		<Button variant="primary" disabled={!folder} busy={saving} onclick={start}
			>Use this folder</Button
		>
	</div>
</div>
