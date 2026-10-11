<script lang="ts">
	// Tool status and installation, shared by Settings → Tools and onboarding (T061).
	import { listen } from '@tauri-apps/api/event';
	import { open as openDialog } from '@tauri-apps/plugin-dialog';
	import { attempt, commands, type Tool, type ToolsStatus } from '#lib/api/index.ts';
	import Badge from './Badge.svelte';
	import Button from './Button.svelte';
	import ProgressBar from './ProgressBar.svelte';

	let { compact = false, onready }: { compact?: boolean; onready?: () => void } = $props();

	let status = $state<ToolsStatus | null>(null);
	let installing = $state(false);
	let step = $state('');

	const names: Record<Tool, string> = { ytDlp: 'yt-dlp', deno: 'Deno', ffmpeg: 'FFmpeg' };
	const purpose: Record<Tool, string> = {
		ytDlp: 'Finds and downloads audio from YouTube.',
		deno: 'Lets yt-dlp read YouTube pages.',
		ffmpeg: 'Optional. Needed only for Opus and MP3 files.'
	};
	const ffmpegHint = navigator.userAgent.includes('Mac')
		? 'Install it with Homebrew: brew install ffmpeg'
		: navigator.userAgent.includes('Windows')
			? 'Install it with winget: winget install ffmpeg'
			: 'Install it from your package manager, e.g. sudo apt install ffmpeg';

	async function load() {
		status = (await attempt(commands.toolsStatus())) ?? null;
		if (status?.ready) onready?.();
	}

	async function install() {
		installing = true;
		const stop = await listen<string>('tools://progress', (e) => (step = e.payload));
		const result = await attempt(commands.installTools());
		stop();
		installing = false;
		step = '';
		if (result) status = result;
		if (result?.ready) onready?.();
	}

	async function choose(tool: Tool) {
		const path = await openDialog({ title: `Choose the ${names[tool]} program` });
		if (typeof path !== 'string' || !status) return;
		const current = Object.fromEntries(
			status.tools.map((t) => [t.tool, t.managed ? null : t.path])
		) as Record<Tool, string | null>;
		current[tool] = path;
		status =
			(await attempt(
				commands.setToolPaths({ ytDlp: current.ytDlp, deno: current.deno, ffmpeg: current.ffmpeg })
			)) ?? status;
	}

	$effect(() => {
		load();
	});
</script>

{#if status}
	<ul class="divide-y divide-line border-y border-line">
		{#each status.tools as t (t.tool)}
			<li class="flex items-center gap-4 py-3">
				<div class="min-w-0 flex-1">
					<p class="font-medium">{names[t.tool]}</p>
					<p class="text-sm text-muted">
						{t.tool === 'ffmpeg' && !t.installed ? ffmpegHint : purpose[t.tool]}
					</p>
					{#if !compact && t.path}<p class="selectable truncate text-xs text-muted">
							{t.path}
						</p>{/if}
				</div>
				{#if t.installed}
					<Badge tone="good">{t.version ?? 'Installed'}</Badge>
				{:else}
					<Badge tone={t.tool === 'ffmpeg' ? 'neutral' : 'bad'}>Not installed</Badge>
				{/if}
				{#if !compact}
					<Button size="sm" variant="ghost" onclick={() => choose(t.tool)}>Choose…</Button>
				{/if}
			</li>
		{/each}
	</ul>
	{#if installing}
		<div class="mt-4"><ProgressBar value={null} label="Installing tools" /></div>
		<p class="mt-2 text-sm text-muted">{step || 'Starting…'}</p>
	{:else if !status.ready}
		<div class="mt-4">
			<Button variant="primary" onclick={install}>Install download tools</Button>
		</div>
		{#if status.message}<p class="mt-2 text-sm text-bad">{status.message}</p>{/if}
	{/if}
{/if}
