<script>
  import { createEventDispatcher, onMount, onDestroy } from 'svelte'
  import { Resolve, SearchYouTube, Download, PickDirectory, GetSavedOutputDir, SaveOutputDir } from '/wailsjs/go/main/App'
  import { EventsOn, EventsOff } from '/wailsjs/runtime/runtime'

  export let pendingURL
  export let searchQuery = ''
  export let resolvedTracks

  const dispatch = createEventDispatcher()

  let tracks = []
  let loading = false
  let error = ''
  let outputDir = ''
  let progressDone = 0
  let progressTotal = 0
  let summaryOk = 0
  let summaryFail = 0
  let downloading = false
  let showProgress = false
  let showSummary = false

  $: selectedTracks = tracks.filter(t => t.selected)
  $: allSelected = tracks.length > 0 && tracks.every(t => t.selected)

  onMount(async () => {
    console.log('[StepFinal] mount — pendingURL:', pendingURL, '| searchQuery:', searchQuery, '| resolvedTracks:', resolvedTracks)

    try {
      outputDir = await GetSavedOutputDir() || ''
      console.log('[StepFinal] GetSavedOutputDir ->', outputDir)
    } catch (e) {
      console.error('[StepFinal] GetSavedOutputDir error:', e)
    }
    if (!outputDir) outputDir = '~/Music'

    if (resolvedTracks && resolvedTracks.length > 0) {
      console.log('[StepFinal] using resolvedTracks, count:', resolvedTracks.length)
      setTracks(resolvedTracks)
    } else if (searchQuery) {
      loading = true
      try {
        const data = await SearchYouTube(searchQuery)
        console.log('[StepFinal] SearchYouTube result:', data)
        if (data.error) error = data.error
        else setTracks(data.tracks || [])
      } catch (err) {
        console.error('[StepFinal] SearchYouTube error:', err)
        error = String(err)
      } finally {
        loading = false
      }
    } else {
      loading = true
      try {
        const data = await Resolve(pendingURL, '', '')
        console.log('[StepFinal] Resolve result:', data)
        if (data.error) error = data.error
        else setTracks(data.tracks || [])
      } catch (err) {
        console.error('[StepFinal] Resolve error:', err)
        error = String(err)
      } finally {
        loading = false
      }
    }

    console.log('[StepFinal] after load — tracks:', tracks.length, '| outputDir:', outputDir)
  })

  onDestroy(() => {
    EventsOff('progress')
    EventsOff('done')
  })

  function setTracks(raw) {
    tracks = raw.map(t => ({ ...t, selected: true, status: 'pending', message: '' }))
  }

  function toggleAll() {
    const next = !allSelected
    tracks = tracks.map(t => ({ ...t, selected: next }))
  }

  function label(t) {
    if (t.artist) return t.artist + ' — ' + t.title
    if (t.title)  return t.title
    return t.url
  }

  function fmtDur(sec) {
    if (!sec) return ''
    const m = Math.floor(sec / 60)
    const s = String(sec % 60).padStart(2, '0')
    return `${m}:${s}`
  }

  async function pickDirectory() {
    const dir = await PickDirectory()
    if (dir) {
      outputDir = dir
      SaveOutputDir(dir)
    }
  }

  function onDirInput(e) {
    outputDir = e.target.value
    SaveOutputDir(outputDir)
  }

  async function startDownload() {
    console.log('[StepFinal] startDownload — outputDir:', outputDir, '| selectedTracks:', selectedTracks.length)
    if (!outputDir.trim() || selectedTracks.length === 0) {
      console.warn('[StepFinal] startDownload blocked — outputDir empty or no tracks selected')
      return
    }

    downloading = true
    showProgress = true
    showSummary = false
    progressDone = 0
    progressTotal = selectedTracks.length

    tracks = tracks.map(t =>
      t.selected ? { ...t, status: 'pending', message: '' } : t
    )

    EventsOn('progress', (ev) => {
      tracks = tracks.map(t =>
        label(t) === ev.track
          ? { ...t, status: ev.status || 'fail', message: ev.message || '' }
          : t
      )
      progressDone++
    })

    EventsOn('done', () => {
      EventsOff('progress')
      EventsOff('done')
      summaryOk   = tracks.filter(t => t.status === 'ok').length
      summaryFail = tracks.filter(t => t.status === 'fail').length
      showSummary = true
      downloading = false
    })

    await Download(selectedTracks, outputDir.trim())
  }
</script>

<div class="step">
  <div class="step-header">
    <button class="back-btn" on:click={() => dispatch('reset')}>&lt;&lt; Start over</button>
    <span class="tracks-count">
      {#if loading}
        Resolving…
      {:else if error}
        &nbsp;
      {:else}
        {selectedTracks.length} / {tracks.length} selected
      {/if}
    </span>
    <a class="gh-link" href="https://github.com/EnricoPDG/music-organizer" target="_blank" title="GitHub">
      <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor"><path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"/></svg>
    </a>
  </div>

  <!-- Select all toggle -->
  {#if !loading && !error && tracks.length > 0}
    <div class="select-bar">
      <label class="select-all">
        <input type="checkbox" checked={allSelected} on:change={toggleAll} />
        Select all
      </label>
    </div>
  {/if}

  <!-- Track list -->
  <div class="track-list">
    {#if loading}
      <div class="loading"><span class="spinner"></span> Resolving tracks…</div>
    {:else if error}
      <div class="loading" style="color:var(--red)">{error}</div>
    {:else}
      {#each tracks as track, i (i)}
        <label class="track">
          <input
            type="checkbox"
            bind:checked={track.selected}
            disabled={downloading}
          />
          <span
            class="badge {track.status}"
            title={track.message}
          >
            {#if track.status === 'ok'}[OK]
            {:else if track.status === 'fail'}[!!]
            {:else}[--]{/if}
          </span>
          <span class="track-name">{label(track)}</span>
          <span class="track-duration">{fmtDur(track.durationSec)}</span>
        </label>
      {/each}
    {/if}
  </div>

  <!-- Progress bar -->
  {#if showProgress}
    <div class="progress-bar">
      <div
        class="progress-fill"
        style="width:{progressTotal ? (progressDone / progressTotal) * 100 : 0}%"
      ></div>
    </div>
  {/if}

  <!-- Summary -->
  {#if showSummary}
    <div class="summary">
      <span class="ok-count">{summaryOk} downloaded</span>  ·
      <span class="fail-count">{summaryFail} failed</span>
    </div>
  {/if}

  <!-- Output dir + download button -->
  <div class="bottom-bar">
    <div class="output-field">
      <label for="output-dir">Output directory</label>
      <div class="dir-input-row">
        <input id="output-dir" type="text" value={outputDir} on:change={onDirInput} placeholder="~/Music" />
        <button class="browse-btn" on:click={pickDirectory} title="Browse">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
        </button>
      </div>
    </div>
    <button
      disabled={downloading || loading || selectedTracks.length === 0 || !outputDir.trim()}
      on:click={startDownload}
    >
      {#if downloading}<span class="spinner"></span>{:else}Download{/if}
    </button>
  </div>
</div>
