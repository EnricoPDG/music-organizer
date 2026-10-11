<script lang="ts">
	import {
		attempt,
		commands,
		type AudioFormat,
		type Theme,
		type ToolsStatus
	} from '#lib/api/index.ts';
	import { settings, updateSettings } from '#lib/stores/settings.svelte.ts';

	let tools = $state<ToolsStatus | null>(null);
	$effect(() => {
		attempt(commands.toolsStatus()).then((t) => (tools = t ?? null));
	});
	const hasFfmpeg = $derived(tools?.tools.find((t) => t.tool === 'ffmpeg')?.installed ?? false);

	const formats: { value: AudioFormat; label: string; note: string; needsFfmpeg: boolean }[] = [
		{
			value: 'm4a',
			label: 'M4A (AAC)',
			note: 'Original quality from YouTube. Plays everywhere.',
			needsFfmpeg: false
		},
		{
			value: 'opus',
			label: 'Opus',
			note: 'Original quality, smaller files. Needs FFmpeg.',
			needsFfmpeg: true
		},
		{
			value: 'mp3',
			label: 'MP3',
			note: 'Converted, so some quality is lost. Needs FFmpeg.',
			needsFfmpeg: true
		}
	];
	const themes: { value: Theme; label: string }[] = [
		{ value: 'system', label: 'Match system' },
		{ value: 'light', label: 'Light' },
		{ value: 'dark', label: 'Dark' }
	];
</script>

{#if settings.value}
	{@const s = settings.value}
	<section aria-labelledby="fmt">
		<h2 id="fmt" class="text-lg font-semibold">Audio format</h2>
		<div class="mt-3 flex flex-col gap-2" role="radiogroup" aria-labelledby="fmt">
			{#each formats as f (f.value)}
				{@const disabled = f.needsFfmpeg && !hasFfmpeg}
				<label
					class="flex cursor-pointer items-start gap-3 rounded-[var(--radius-control)] border border-line px-3 py-2.5 has-[:checked]:border-plum has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-50"
				>
					<input
						type="radio"
						name="format"
						class="mt-1 accent-[var(--plum)]"
						checked={s.audioFormat === f.value}
						{disabled}
						onchange={() => updateSettings({ audioFormat: f.value })}
					/>
					<span>
						<span class="font-medium">{f.label}</span>
						<span class="block text-sm text-muted">{f.note}</span>
					</span>
				</label>
			{/each}
		</div>
	</section>

	<section class="mt-10" aria-labelledby="look">
		<h2 id="look" class="text-lg font-semibold">Appearance</h2>
		<div
			class="mt-3 inline-flex rounded-[var(--radius-control)] bg-raised p-1"
			role="radiogroup"
			aria-labelledby="look"
		>
			{#each themes as t (t.value)}
				<button
					role="radio"
					aria-checked={s.theme === t.value}
					class="rounded-[6px] px-3 py-1.5 text-sm text-muted transition-colors aria-checked:bg-surface aria-checked:text-ink aria-checked:shadow-sm"
					onclick={() => updateSettings({ theme: t.value })}>{t.label}</button
				>
			{/each}
		</div>
		<label class="mt-4 flex items-center gap-3">
			<input
				type="checkbox"
				class="accent-[var(--plum)]"
				checked={s.reduceMotion === true}
				onchange={(e) =>
					updateSettings(
						e.currentTarget.checked ? { reduceMotion: true } : { followSystemMotion: true }
					)}
			/>
			Reduce motion
		</label>
	</section>

	<section class="mt-10" aria-labelledby="dl">
		<h2 id="dl" class="text-lg font-semibold">Downloads</h2>
		<label class="mt-3 flex items-center gap-3">
			Download
			<select
				class="rounded-[var(--radius-control)] border border-line bg-surface px-2 py-1"
				value={s.maxParallelDownloads}
				onchange={(e) => updateSettings({ maxParallelDownloads: Number(e.currentTarget.value) })}
			>
				{#each [1, 2, 3, 4, 5] as n (n)}<option value={n}>{n}</option>{/each}
			</select>
			tracks at a time
		</label>
		<label class="mt-3 flex items-center gap-3">
			<input
				type="checkbox"
				class="accent-[var(--plum)]"
				checked={s.reviewLowConfidence}
				onchange={(e) => updateSettings({ reviewLowConfidence: e.currentTarget.checked })}
			/>
			Ask me before downloading uncertain matches
		</label>
		<p class="mt-4 text-sm text-muted">
			Files are named <code class="selectable rounded bg-raised px-1.5 py-0.5"
				>{s.namingPattern}</code
			>
		</p>
	</section>
{/if}
