<script lang="ts">
  import { onMount } from 'svelte'
  import Toasts from './components/Toasts.svelte'
  import { api } from './lib/api'
  import { onAction, setWebGamepad } from './lib/input'
  import { initLibrary, library, refresh } from './lib/library.svelte'
  import { back, current, nav, reset } from './lib/nav.svelte'
  import { toast } from './lib/toast.svelte'
  import Detail from './screens/Detail.svelte'
  import Games from './screens/Games.svelte'
  import Platforms from './screens/Platforms.svelte'
  import Search from './screens/Search.svelte'
  import Settings from './screens/Settings.svelte'
  import Setup from './screens/Setup.svelte'

  let ready = $state(false)
  let server = $state('')

  // Registered first, so it only sees what the screens leave alone.
  onAction(({ action }) => {
    if (action === 'b') return back()
    return false
  })

  function toSetup() {
    reset({ name: 'setup' })
  }

  function signedIn() {
    reset({ name: 'platforms' })
    refresh()
  }

  onMount(async () => {
    initLibrary(toSetup)
    try {
      const s = await api.status()
      server = s.server_url ?? ''
      library.lastSync = s.last_sync_at
      library.refreshing = s.refreshing
      const settings = await api.settings()
      setWebGamepad(settings.web_gamepad)
      if (s.server_url && s.signed_in) {
        // Show the cache immediately, then sync in the background.
        reset({ name: 'platforms' })
        refresh()
      } else {
        toSetup()
      }
      ready = true

      const notes = await api.whatsNew().catch(() => null)
      if (notes !== null) toast(`Updated to ${s.version}.`, 'info', 5000)
      if (settings.check_updates && s.updatable) {
        const r = await api.checkUpdate().catch(() => null)
        if (r) toast(`Version ${r.version} is available. Install it from Settings (☰).`, 'info', 7000)
      }
    } catch (e) {
      ready = true
      toast(String(e), 'error')
      toSetup()
    }
  })

  const top = $derived(current())
</script>

<div class="app">
  {#if ready && top}
    {#key nav.stack.length + ':' + top.name}
      {#if top.name === 'setup'}
        <Setup initialServer={library.needsSignIn ? server : ''} ondone={signedIn} />
      {:else if top.name === 'platforms'}
        <Platforms entry={top} />
      {:else if top.name === 'games'}
        <Games platformId={top.platformId} platformName={top.platformName} entry={top} />
      {:else if top.name === 'detail'}
        <Detail romId={top.romId} siblings={top.siblings} />
      {:else if top.name === 'search'}
        <Search entry={top} />
      {:else if top.name === 'settings'}
        <Settings onsignout={toSetup} />
      {/if}
    {/key}
  {/if}
</div>
<Toasts />
