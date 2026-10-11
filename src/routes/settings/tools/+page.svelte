<script lang="ts">
	import ToolsPanel from '#lib/components/ToolsPanel.svelte';
</script>

<p class="max-w-prose text-muted">
	These programs do the downloading. The app installs and updates yt-dlp and Deno for you, and
	checks each download against a known checksum. You can also point the app at copies you installed
	yourself.
</p>
<div class="mt-6"><ToolsPanel /></div>
