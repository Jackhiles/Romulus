<script lang="ts">
  import { onDestroy, untrack } from 'svelte'
  import Confirm from '../components/Confirm.svelte'
  import Footer from '../components/Footer.svelte'
  import GameBrowser from '../components/GameBrowser.svelte'
  import Header from '../components/Header.svelte'
  import { api, games, type DownloadAllPlan, type GamesView, type Platform, type Rom, type Sort } from '../lib/api'
  import { hueOf, size } from '../lib/format'
  import { toast } from '../lib/toast.svelte'
  import { onAction, type Action } from '../lib/input'
  import { INSTALLED, gamesOf, isCollection, systemsWithCollections } from '../lib/collections'
  import { library } from '../lib/library.svelte'
  import { device } from '../lib/play.svelte'
  import { push, type Screen } from '../lib/nav.svelte'
  import { GAMES_VIEW_LABEL, isGamesView, nextGamesView } from '../lib/views'

  let { entry = $bindable() }: { entry: Screen } = $props()
  // App only mounts this screen for a 'games' entry.
  const g = $derived(entry as Extract<Screen, { name: 'games' }>)

  let roms = $state<Rom[]>([])
  // svelte-ignore state_referenced_locally
  let index = $state(g.index ?? 0)
  // svelte-ignore state_referenced_locally
  let sort = $state<Sort>(g.sort ?? 'title')
  let view = $state<GamesView>('list')
  let views: Record<string, string> = {}
  const viewFor = (slug: string): GamesView => {
    const v = views[slug]
    return isGamesView(v) ? v : 'list'
  }
  let platforms: Platform[] = []
  // Collections mix systems, so their rows name the system.
  let slugOf = $state<Record<number, string>>({})
  api.platforms().then((ps) => (slugOf = Object.fromEntries(ps.map((p) => [p.id, p.slug.toUpperCase()]))))

  $effect(() => {
    g.index = index
    g.sort = sort
  })

  api.settings().then((s) => {
    views = s.games_view ?? {}
    view = viewFor(g.platformSlug)
  })
  systemsWithCollections(0).then((s) => (platforms = s.list))

  $effect(() => {
    void library.revision
    void device.revision
    const pid = g.platformId
    const order = sort
    // Keep focus on the same game across reloads (read untracked, or this effect would
    // re-run every time it loads).
    const keep = untrack(() => roms[index]?.id)
    gamesOf(pid, order).then((r) => {
      if (pid !== g.platformId) return
      roms = r
      const i = keep ? r.findIndex((x) => x.id === keep) : -1
      index = i >= 0 ? i : Math.min(index, Math.max(r.length - 1, 0))
    })
  })

  /** ES-DE-style: left/right in the list moves to the previous/next system. */
  function switchSystem(delta: 1 | -1) {
    const n = platforms.length
    if (n < 2) return
    const i = platforms.findIndex((p) => p.id === g.platformId)
    const p = platforms[(i + delta + n) % n]
    roms = []
    index = 0
    g.platformId = p.id
    g.platformName = p.name
    g.platformSlug = p.slug
    view = viewFor(p.slug)
  }

  // "Download all" on a collection (not "On this Deck": those are here already).
  const canDownloadAll = $derived(isCollection(g.platformId) && g.platformId !== INSTALLED)
  let planAll = $state<DownloadAllPlan | null>(null)

  async function askDownloadAll() {
    try {
      const p = await games.planDownloadAll(roms.map((r) => r.id))
      if (p.rom_ids.length === 0) toast('Every game here is on this Deck or downloading already.')
      else if (!p.fits)
        toast(`Not enough room: ${p.rom_ids.length} games need ${size(p.bytes)}, ${size(p.free_bytes)} is free.`, 'error', 8000)
      else planAll = p
    } catch (e) {
      toast(String(e), 'error', 6000)
    }
  }

  async function downloadAll(p: DownloadAllPlan) {
    planAll = null
    try {
      const n = await games.downloadAll(p.rom_ids)
      toast(`${n} ${n === 1 ? 'game' : 'games'} added to Downloads`)
    } catch (e) {
      toast(String(e), 'error', 6000)
    }
  }

  function open(i: number) {
    const r = roms[i]
    if (r) push({ name: 'detail', romId: r.id, siblings: roms.map((x) => x.id) })
  }

  const off = onAction(({ action }) => {
    if (planAll) return false
    switch (action) {
      // Reached only when the browser doesn't use them: left/right in the list,
      // up/down in the carousel.
      case 'left':
      case 'right':
      case 'up':
      case 'down':
        switchSystem(action === 'right' || action === 'down' ? 1 : -1)
        return true
      case 'a':
        open(index)
        return true
      case 'x':
        sort = sort === 'title' ? 'newest' : 'title'
        index = 0
        return true
      case 'select':
        view = nextGamesView(view)
        views[g.platformSlug] = view
        api.setGamesView(g.platformSlug, view).catch(() => {})
        return true
      case 'y':
        push({ name: 'search' })
        return true
      case 'start':
        if (canDownloadAll) askDownloadAll()
        else push({ name: 'settings' })
        return true
    }
    return false
  })
  onDestroy(off)

  const hints = $derived<[Action, string][]>([
    ['a', 'Details'],
    ...(view === 'list' ? ([['right', 'System']] as [Action, string][]) : []),
    ...(view === 'carousel' ? ([['down', 'System']] as [Action, string][]) : []),
    ['x', sort === 'title' ? 'Newest first' : 'A–Z'],
    ['select', GAMES_VIEW_LABEL[nextGamesView(view)]],
    ['y', 'Search'],
    ...(canDownloadAll && roms.length ? ([['start', 'Download all']] as [Action, string][]) : []),
    ['b', 'Back'],
  ])
</script>

<Header title={g.platformName} subtitle={`${roms.length} games${sort === 'newest' ? ' · newest first' : ''}`} />
<GameBrowser
  {roms}
  bind:index
  {view}
  hue={hueOf(g.platformSlug)}
  letters={sort === 'title' && !isCollection(g.platformId)}
  onopen={open}
  tag={isCollection(g.platformId) ? (r) => slugOf[r.platform_id] ?? '' : undefined}
/>
<Footer {hints} />
{#if planAll}
  <Confirm
    title="Download {planAll.rom_ids.length} {planAll.rom_ids.length === 1 ? 'game' : 'games'}?"
    message="About {size(planAll.bytes)} from RomM; {size(planAll.free_bytes)} is free. They download one after another, and continue after a restart."
    confirm="Download all"
    onconfirm={() => planAll && downloadAll(planAll)}
    oncancel={() => (planAll = null)}
  />
{/if}
