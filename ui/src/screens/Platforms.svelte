<script lang="ts">
  // Choosing a system. Three layouts, picked with Select or in Settings:
  //  - carousel: one system large in the middle, its neighbours either side (default)
  //  - list: systems down the left, the focused one previewed on the right
  //  - grid: cards for every system
  import { onDestroy } from 'svelte'
  import Backdrop from '../components/Backdrop.svelte'
  import Cover from '../components/Cover.svelte'
  import Footer from '../components/Footer.svelte'
  import Header from '../components/Header.svelte'
  import VirtualGrid from '../components/VirtualGrid.svelte'
  import VirtualList from '../components/VirtualList.svelte'
  import { api, music, type Platform, type SystemsView } from '../lib/api'
  import { APP_NAME } from '../lib/app'
  import { hueOf } from '../lib/format'
  import { page, step } from '../lib/grid'
  import { onAction, type Action } from '../lib/input'
  import { systemsWithCollections } from '../lib/collections'
  import { library, refresh } from '../lib/library.svelte'
  import { device } from '../lib/play.svelte'
  import { push, type Screen } from '../lib/nav.svelte'

  let { entry = $bindable() }: { entry: Screen } = $props()

  const VIEWS: SystemsView[] = ['carousel', 'list', 'grid']

  let platforms = $state<Platform[]>([])
  let covers = $state<Record<string, string[]>>({})
  // svelte-ignore state_referenced_locally
  let index = $state(entry.index ?? 0)
  let view = $state<SystemsView>('carousel')
  let cols = $state(1)
  let rows = $state(1)
  let loaded = $state(false)

  $effect(() => {
    entry.index = index
  })

  api.settings().then((s) => {
    if (VIEWS.includes(s.systems_view)) view = s.systems_view
  })

  $effect(() => {
    void library.revision
    void device.revision
    systemsWithCollections(6).then(({ list: p, covers: c }) => {
      platforms = p
      covers = c
      index = Math.min(index, Math.max(p.length - 1, 0))
      loaded = true
    })
  })

  const focused = $derived(platforms[index])
  const hue = $derived(focused ? hueOf(focused.slug) : 220)
  const total = $derived(platforms.filter((p) => p.id > 0).reduce((s, p) => s + p.rom_count, 0))
  const systemCount = $derived(platforms.filter((p) => p.id > 0).length)

  function open(i: number) {
    const p = platforms[i]
    if (p) push({ name: 'games', platformId: p.id, platformName: p.name, platformSlug: p.slug })
  }

  function move(delta: number) {
    const n = platforms.length
    if (n) index = Math.min(Math.max(index + delta, 0), n - 1)
  }

  const off = onAction(({ action }) => {
    const n = platforms.length
    switch (action) {
      case 'left':
      case 'right':
        if (view === 'carousel') move(action === 'right' ? 1 : -1)
        else if (view === 'grid') index = step(index, action, cols, n)
        return true
      case 'up':
      case 'down':
        if (view === 'list') move(action === 'down' ? 1 : -1)
        else if (view === 'grid') index = step(index, action, cols, n)
        return true
      case 'lb':
      case 'rb': {
        const d = action === 'rb' ? 1 : -1
        if (view === 'grid') index = page(index, d, cols, rows, n)
        else move(d * (view === 'list' ? rows : 5))
        return true
      }
      case 'a':
        open(index)
        return true
      case 'select':
        view = VIEWS[(VIEWS.indexOf(view) + 1) % VIEWS.length]
        api.updateSettings({ systems_view: view }).catch(() => {})
        return true
      case 'y':
        push({ name: 'search' })
        return true
      case 'x':
        refresh()
        return true
      case 'start':
        push({ name: 'settings' })
        return true
    }
    return false
  })
  onDestroy(off)
  // The systems screen is quiet.
  music.stop().catch(() => {})

  const NEXT_VIEW: Record<SystemsView, string> = { carousel: 'List', list: 'Grid', grid: 'Carousel' }
  const hints = $derived<[Action, string][]>([
    ['a', 'Open'],
    ['y', 'Search'],
    ['x', 'Sync'],
    ['select', NEXT_VIEW[view]],
    ['start', 'Settings'],
  ])

  // Carousel: only the focused card and three either side are in the DOM.
  const SPACING = 430
  const near = $derived.by(() => {
    const out: number[] = []
    for (let i = Math.max(0, index - 3); i <= Math.min(platforms.length - 1, index + 3); i++) out.push(i)
    return out
  })
</script>

<div class="screen">
  <Backdrop {hue} />
  <Header title={APP_NAME} subtitle={total ? `${systemCount} systems · ${total} games` : ''} />

  {#if loaded && platforms.length === 0}
    <div class="empty">
      {#if library.refreshing}Loading your library…{:else}No games yet. Press <strong>X</strong> to sync with RomM.{/if}
    </div>
  {:else if view === 'carousel'}
    <main class="carousel">
      <div class="track">
        {#each near as i (platforms[i].id)}
          {@const p = platforms[i]}
          {@const d = i - index}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="card"
            class:focused={d === 0}
            role="button"
            tabindex="-1"
            style:--hue={hueOf(p.slug)}
            style:transform="translate3d(calc(-50% + {d * SPACING}px), -50%, 0) scale({d === 0 ? 1 : 0.74})"
            style:opacity={d === 0 ? 1 : Math.abs(d) === 1 ? 0.5 : 0.18}
            onclick={() => (d === 0 ? open(i) : (index = i))}
          >
            <div class="abbr">{p.slug.toUpperCase()}</div>
            <div class="fan">
              {#each (covers[p.id] ?? []).slice(0, 3) as c, k (c)}
                <div class="fan-cover" style:transform="translateX({k * -46}px) rotate({(k - 1) * 6}deg)" style:z-index={3 - k}>
                  <Cover path={c} title="" />
                </div>
              {/each}
            </div>
            <div class="name">{p.name}</div>
            <div class="count">{p.rom_count} {p.rom_count === 1 ? 'game' : 'games'}</div>
          </div>
        {/each}
      </div>
      <div class="position">{index + 1} / {platforms.length}</div>
    </main>
  {:else if view === 'list'}
    <main class="list">
      <div class="names">
        <VirtualList items={platforms} bind:index bind:rows onactivate={open}>
          {#snippet row(p: Platform, _i: number, isFocused: boolean)}
            <div class="row" class:focused={isFocused}>
              <span>{p.name}</span><span class="n">{p.rom_count}</span>
            </div>
          {/snippet}
        </VirtualList>
      </div>
      {#if focused}
        <div class="preview" style:--hue={hue}>
          <div class="abbr big">{focused.slug.toUpperCase()}</div>
          <h2>{focused.name}</h2>
          <div class="count">{focused.rom_count} {focused.rom_count === 1 ? 'game' : 'games'}</div>
          <div class="shelf">
            {#each (covers[focused.id] ?? []).slice(0, 6) as c (c)}
              <div class="shelf-cover"><Cover path={c} title="" /></div>
            {/each}
          </div>
        </div>
      {/if}
    </main>
  {:else}
    <main class="grid">
      <VirtualGrid items={platforms} bind:index bind:cols bind:rows tileWidth={268} tileHeight={150} gap={24} onactivate={open}>
        {#snippet tile(p: Platform, _i: number, isFocused: boolean)}
          <div class="tile" class:focused={isFocused} style:--hue={hueOf(p.slug)}>
            <div class="abbr">{p.slug.toUpperCase()}</div>
            <div class="name small">{p.name}</div>
            <div class="count">{p.rom_count} {p.rom_count === 1 ? 'game' : 'games'}</div>
          </div>
        {/snippet}
      </VirtualGrid>
    </main>
  {/if}

  <Footer {hints} />
</div>

<style>
  .screen {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  main {
    position: relative;
    z-index: 1;
    flex: 1;
    min-height: 0;
  }
  .empty {
    position: relative;
    z-index: 1;
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.3em;
    font-size: 20px;
    color: var(--muted);
  }
  .abbr {
    font-size: 13px;
    font-weight: 800;
    letter-spacing: 0.2em;
    color: hsl(var(--hue, 220) 70% 75%);
  }
  .name {
    font-size: 32px;
    font-weight: 800;
    line-height: 1.1;
    letter-spacing: -0.01em;
  }
  .name.small {
    font-size: 19px;
  }
  .count {
    color: var(--muted);
    font-size: 15px;
  }

  /* ---- carousel ---- */
  .carousel {
    display: flex;
    flex-direction: column;
  }
  .track {
    position: relative;
    flex: 1;
    overflow: hidden;
  }
  .card {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 440px;
    height: 300px;
    padding: 24px 28px;
    border-radius: 22px;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    gap: 6px;
    background: linear-gradient(155deg, hsl(var(--hue) 38% 24%), hsl(var(--hue) 30% 11%));
    border: 1px solid rgb(255 255 255 / 0.08);
    box-shadow: 0 12px 30px rgb(0 0 0 / 0.45);
    transition:
      transform 240ms cubic-bezier(0.2, 0.8, 0.2, 1),
      opacity 240ms;
    will-change: transform, opacity;
  }
  .card.focused {
    border-color: hsl(var(--hue) 60% 60% / 0.6);
  }
  .card .abbr {
    position: absolute;
    top: 22px;
    left: 28px;
  }
  .fan {
    position: absolute;
    top: 26px;
    right: 26px;
    width: 120px;
    height: 160px;
  }
  .fan-cover {
    position: absolute;
    inset: 0;
    transform-origin: bottom center;
  }
  .position {
    text-align: center;
    color: var(--muted);
    font-size: 14px;
    font-variant-numeric: tabular-nums;
    padding-bottom: 6px;
  }

  /* ---- list ---- */
  .list {
    display: flex;
  }
  .names {
    width: 40%;
    max-width: 480px;
    padding: 4px 0 4px 24px;
  }
  .row {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 16px;
    margin-right: 12px;
    border-radius: 8px;
    font-size: 17px;
    color: var(--text-2);
  }
  .row span:first-child {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row .n {
    color: var(--muted);
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }
  .row.focused {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .row.focused .n {
    color: rgb(255 255 255 / 0.8);
  }
  .preview {
    flex: 1;
    min-width: 0;
    padding: 30px 44px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .preview h2 {
    margin: 0;
    font-size: 38px;
    line-height: 1.1;
  }
  .shelf {
    margin-top: 28px;
    display: grid;
    grid-template-columns: repeat(3, 130px);
    gap: 16px;
  }
  .shelf-cover {
    width: 130px;
    height: 174px;
  }

  /* ---- grid ---- */
  .grid {
    padding: 0 20px;
  }
  .tile {
    width: 100%;
    height: 100%;
    border-radius: 14px;
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    gap: 4px;
    position: relative;
    background: linear-gradient(150deg, hsl(var(--hue) 35% 22%), hsl(var(--hue) 28% 12%));
    border: 1px solid rgb(255 255 255 / 0.06);
    transition: transform 110ms ease-out;
  }
  .tile::after {
    content: '';
    position: absolute;
    inset: -1px;
    border-radius: inherit;
    box-shadow: 0 0 0 3px var(--accent);
    opacity: 0;
    transition: opacity 110ms ease-out;
    pointer-events: none;
  }
  .tile.focused::after {
    opacity: 1;
  }
  .tile .abbr {
    position: absolute;
    top: 14px;
    left: 18px;
  }
  .tile.focused {
    transform: scale(1.05);
  }
</style>
