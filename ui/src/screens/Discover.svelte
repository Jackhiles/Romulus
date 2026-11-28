<script lang="ts">
  // Discovery: games one at a time in random order, to find something to play. A opens
  // the game, Y keeps it (adds it to Favourites) and moves on, X/right skips, left goes
  // back. Select changes which games it draws from.
  import { onDestroy, untrack } from 'svelte'
  import { fly } from 'svelte/transition'
  import Backdrop from '../components/Backdrop.svelte'
  import Cover from '../components/Cover.svelte'
  import Footer from '../components/Footer.svelte'
  import GameInfo from '../components/GameInfo.svelte'
  import Header from '../components/Header.svelte'
  import { api, collections, music, type DiscoverPool } from '../lib/api'
  import { FocusedDetail } from '../lib/details.svelte'
  import { hueOf } from '../lib/format'
  import { onAction, type Action } from '../lib/input'
  import { push, type Screen } from '../lib/nav.svelte'
  import { device } from '../lib/play.svelte'
  import { toast } from '../lib/toast.svelte'

  let { entry = $bindable() }: { entry: Screen } = $props()
  const e = $derived(entry as Extract<Screen, { name: 'discover' }>)

  const POOLS: DiscoverPool[] = ['all', 'installed', 'unplayed']
  const POOL_LABEL: Record<DiscoverPool, string> = {
    all: 'Whole library',
    installed: 'On this Deck',
    unplayed: 'Never played here',
  }

  // Where this screen was before opening a game (restored on Back).
  const saved = untrack(() => entry as Extract<Screen, { name: 'discover' }>)
  const resumed = (saved.ids?.length ?? 0) > 0
  let pool = $state<DiscoverPool>(saved.pool ?? 'all')
  let ids = $state<number[]>(saved.ids ?? [])
  let index = $state(saved.index ?? 0)
  let loaded = $state(resumed)
  let dir = $state(1)
  let keeping = $state(false)

  $effect(() => {
    e.pool = pool
    e.ids = ids
    e.index = index
  })

  function load(p: DiscoverPool, append = false) {
    api
      .discover(p)
      .then((list) => {
        if (p !== pool) return
        if (append) {
          // A fresh shuffle once the list runs out, without repeating the current game.
          const cur = ids[index]
          ids = [...ids, ...list.filter((id) => id !== cur)]
        } else {
          ids = list
          index = 0
        }
        loaded = true
      })
      .catch((err) => {
        loaded = true
        toast(String(err), 'error')
      })
  }
  if (!resumed) load(saved.pool ?? 'all')

  const focus = new FocusedDetail(0)
  const id = $derived(ids[index])
  $effect(() => {
    focus.set(id)
  })
  const d = $derived(focus.detail && focus.detail.id === id ? focus.detail : null)
  const platform = $derived(typeof d?.data?.platform_display_name === 'string' ? d.data.platform_display_name : '')
  const slug = $derived(typeof d?.data?.platform_slug === 'string' ? d.data.platform_slug : '')

  // The game's music, once it has been on screen a moment.
  let musicTimer: ReturnType<typeof setTimeout> | undefined
  $effect(() => {
    const want = id
    clearTimeout(musicTimer)
    if (want !== undefined) musicTimer = setTimeout(() => music.preview(want).catch(() => {}), 900)
  })

  function move(delta: 1 | -1) {
    const next = index + delta
    if (next < 0) return
    dir = delta
    index = next
    // Top up before the end.
    if (ids.length - index < 5) untrack(() => load(pool, true))
  }

  async function keep() {
    const want = id
    if (want === undefined || keeping) return
    keeping = true
    try {
      if (!(await collections.isFavourite(want))) await collections.setFavourite(want, true)
      toast(`★ ${d?.title ?? 'Game'} kept in Favourites`)
      if (ids[index] === want) move(1)
    } catch (err) {
      toast(String(err), 'error', 8000)
    } finally {
      keeping = false
    }
  }

  const off = onAction(({ action }) => {
    switch (action) {
      case 'a':
        if (id !== undefined) push({ name: 'detail', romId: id })
        return true
      case 'x':
      case 'right':
        move(1)
        return true
      case 'left':
        move(-1)
        return true
      case 'y':
        keep()
        return true
      case 'select':
        pool = POOLS[(POOLS.indexOf(pool) + 1) % POOLS.length]
        loaded = false
        ids = []
        index = 0
        load(pool)
        return true
    }
    return false
  })
  onDestroy(() => {
    off()
    clearTimeout(musicTimer)
    focus.destroy()
  })

  const hints = $derived<[Action, string][]>([
    ['a', 'Details'],
    ['y', 'Keep'],
    ['x', 'Next'],
    ...(index > 0 ? ([['left', 'Previous']] as [Action, string][]) : []),
    ['select', POOL_LABEL[POOLS[(POOLS.indexOf(pool) + 1) % POOLS.length]]],
    ['b', 'Back'],
  ])
</script>

<div class="screen">
  <Backdrop src={focus.id === id ? focus.shot : null} hue={hueOf(slug || 'x')} strength={0.45} />
  <Header title="Discover" subtitle={POOL_LABEL[pool]} />

  <main>
    {#if loaded && ids.length === 0}
      <div class="empty">No games here. Press <strong>⧉</strong> to look somewhere else.</div>
    {:else if d}
      {#key d.id}
        <div class="game" in:fly={{ x: 70 * dir, duration: 240, opacity: 0 }}>
          <div class="box">
            <Cover path={d.cover_large ?? d.cover_small} title={d.title} focused />
          </div>
          <div class="text">
            <div class="system">
              {platform}
              {#if device.installed.has(d.id)}<span class="here">On this Deck</span>{/if}
            </div>
            <GameInfo title={d.title} detail={d} lines={7} size="lg" />
          </div>
        </div>
      {/key}
    {/if}
  </main>

  <Footer {hints} />
</div>

<style>
  .screen {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  main {
    position: relative;
    z-index: 1;
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    padding: 0 64px;
  }
  .game {
    display: flex;
    align-items: center;
    gap: 52px;
    width: 100%;
  }
  .box {
    flex: none;
    width: 330px;
    height: 440px;
    filter: drop-shadow(0 18px 34px rgb(0 0 0 / 0.55));
  }
  .text {
    flex: 1;
    min-width: 0;
    max-width: 720px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .system {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .here {
    font-size: 12px;
    letter-spacing: 0.04em;
    text-transform: none;
    padding: 2px 10px;
    border-radius: 999px;
    background: var(--ok);
    color: #fff;
  }
  .empty {
    margin: 0 auto;
    color: var(--text-2);
    font-size: 18px;
  }
</style>
