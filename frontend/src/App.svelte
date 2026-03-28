<script>
  import StepURL from './lib/StepURL.svelte'
  import StepSpotify from './lib/StepSpotify.svelte'
  import StepFinal from './lib/StepFinal.svelte'

  let currentStep = 'url'
  let pendingURL = ''
  let searchQuery = ''
  let resolvedTracks = []

  function handleContinue(e) {
    if (e.detail.isSearch) {
      searchQuery = e.detail.url
      pendingURL = ''
      resolvedTracks = []
      currentStep = 'final'
      return
    }

    pendingURL = e.detail.url
    searchQuery = ''
    resolvedTracks = []

    if (e.detail.isSpotify) {
      currentStep = 'spotify'
    } else {
      currentStep = 'final'
    }
  }

  function handleResolved(e) {
    resolvedTracks = e.detail.tracks
    currentStep = 'final'
  }

  function handleReset() {
    pendingURL = ''
    searchQuery = ''
    resolvedTracks = []
    currentStep = 'url'
  }
</script>

<div class="container">
  {#if currentStep === 'url'}
    <StepURL on:continue={handleContinue} />
  {:else if currentStep === 'spotify'}
    <StepSpotify
      {pendingURL}
      on:resolved={handleResolved}
      on:back={() => (currentStep = 'url')}
    />
  {:else}
    <StepFinal
      {pendingURL}
      {searchQuery}
      {resolvedTracks}
      on:reset={handleReset}
    />
  {/if}
</div>
