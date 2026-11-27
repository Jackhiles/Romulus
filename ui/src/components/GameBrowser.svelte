<script lang="ts">
  // A system's games (or search results) in one of four layouts:
  //  - list: titles with a detail panel
  //  - grid / covers: a cover grid (small or big covers), the focused game across the top
  //  - carousel: one row of covers, the focused one large in the middle
  // Handles moving the focus; the screen using it handles A, X, Y, Select, left/right
  // in the list and up/down in the carousel.
  import { onDestroy } from 'svelte'
  import { music, type GamesView, type Rom } from '../lib/api'
  import { FocusedDetail } from '../lib/details.svelte'
  import { fileTags } from '../lib/format'
  import { device } from '../lib/play.svelte'
  import { jumpLetter, letterOf, page, step } from '../lib/grid'
  import { around, flowOffset } from '../lib/views'
  import { onAction } from '../lib/input'
  import Backdrop from './Backdrop.svelte'
  import Cover from './Cover.svelte'
  import GameInfo from './GameInfo.svelte'
  import VirtualGrid from './VirtualGrid.svelte'
  import VirtualList from './VirtualList.svelte'

  let {
    roms,
    index = $bindable(0),
    view,
    hue = 220,
    active = true,
    letters = true,
    onopen,
    ontop,
    tag,
  }: {
    roms: Rom[]
    index: number
    view: GamesView
    hue?: number
    /** False while something else (a search field) has the controller. */
    active?: boolean
    /** LT/RT jump by first letter (only when sorted by title). */
    letters?: boolean
    onopen: (i: number) => void
    /** Up was pressed on the first row. */
    ontop?: () => void
    /** Dim text after a title in the list. Default: file-name tags for titles that
     *  appear more than once (discs, regions, revisions). */
    tag?: (r: Rom) => string
  } = $props()

  const repeated = $derived.by(() => {
    const seen = new Set<string>()
    const twice = new Set<string>()
    for (const r of roms) (seen.has(r.title) ? twice : seen).add(r.title)
    return twice
  })
  const tagOf = (r: Rom) =>
    tag
      ? tag(r)
      : r.versions
        ? `${r.versions + 1} versions`
        : repeated.has(r.title)
          ? fileTags(r.fs_name)
          : ''

  let cols = $state(1)
  let rows = $state(1)
  let letter = $state<string | null>(null)
  let letterTimer: ReturnType<typeof setTimeout> | undefined

  const focus = new FocusedDetail()
  const rom = $derived(roms[index])
  $effect(() => {
    focus.set(rom?.id)
  })

  // Music for the focused game, once focus has rested a moment.
  let musicTimer: ReturnType<typeof setTimeout> | undefined
  $effect(() => {
    const id = rom?.id
    clearTimeout(musicTimer)
    if (id === undefined || !active) return
    musicTimer = setTimeout(() => music.preview(id).catch(() => {}), 1200)
  })

  function showLetter() {
    letter = letterOf(roms[index]?.title ?? '')
    clearTimeout(letterTimer)
    letterTimer = setTimeout(() => (letter = null), 650)
  }

  const clamp = (i: number) => Math.min(Math.max(i, 0), Math.max(roms.length - 1, 0))
  const FLOW_PAGE = 5

  const off = onAction(({ action }) => {
    if (!active) return false
    const n = roms.length
    if (view === 'carousel') {
      switch (action) {
        case 'left':
        case 'right':
          index = clamp(index + (action === 'right' ? 1 : -1))
          return true
        case 'up':
          if (ontop) {
            ontop()
            return true
          }
          return false
        case 'down':
          return false
        case 'lb':
        case 'rb':
          index = clamp(index + (action === 'rb' ? FLOW_PAGE : -FLOW_PAGE))
          return true
      }
    }
    const perPage = view === 'list' ? rows : view === 'carousel' ? FLOW_PAGE : cols * rows
    switch (action) {
      case 'up':
      case 'down':
        if (action === 'up' && ontop && (view === 'list' ? index === 0 : index < cols)) {
          ontop()
          return true
        }
        index = view === 'list' ? Math.min(Math.max(index + (action === 'down' ? 1 : -1), 0), Math.max(n - 1, 0)) : step(index, action, cols, n)
        return true
      case 'left':
      case 'right':
        if (view === 'list') return false
        index = step(index, action, cols, n)
        return true
      case 'lb':
      case 'rb':
        index = view === 'list' ? Math.min(Math.max(index + (action === 'rb' ? perPage : -perPage), 0), Math.max(n - 1, 0)) : page(index, action === 'rb' ? 1 : -1, cols, rows, n)
        return true
      case 'lt':
      case 'rt':
        if (letters) {
          index = jumpLetter(roms.map((r) => r.title), index, action === 'rt' ? 1 : -1)
          showLetter()
        } else {
          index = clamp(index + (action === 'rt' ? 3 : -3) * perPage)
        }
        return true
    }
    return false
  })
  onDestroy(() => {
    off()
    clearTimeout(musicTimer)
    focus.destroy()
    clearTimeout(letterTimer)
  })
</script>

<div class="browser view-{view}">
  <Backdrop src={focus.shot} {hue} strength={view === 'list' ? 0.22 : 0.3} />

  {#if view === 'list'}
    <div class="list">
      <VirtualList items={roms} bind:index bind:rows onactivate={onopen}>
        {#snippet row(r: Rom, _i: number, focused: boolean)}
          {@const t = tagOf(r)}
          <div class="row" class:focused class:dim={!active}>
            <span class="t">{r.title}</span>{#if t}<span class="tag">{t}</span>{/if}
            {#if device.installed.has(r.id)}<span class="here" title="On this Deck"></span>{/if}
          </div>
        {/snippet}
      </VirtualList>
    </div>
    <div class="panel">
      {#if rom}
        <div class="art">
          <div class="shot" class:empty={!focus.shot}>
            {#if focus.shot}<img src={focus.shot} alt="" />{/if}
          </div>
          <div class="box"><Cover path={rom.cover_large ?? rom.cover_small} title={rom.title} /></div>
        </div>
        <GameInfo title={rom.title} detail={focus.detail?.id === rom.id ? focus.detail : null} lines={6} />
      {/if}
    </div>
  {:else if view === 'carousel'}
    <div class="flow">
      <div class="strip">
        {#each around(index, roms.length, 5) as i (roms[i].id)}
          {@const r = roms[i]}
          {@const d = i - index}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="card"
            role="button"
            tabindex="-1"
            style:z-index={10 - Math.abs(d)}
            style:transform="translate3d(calc(-50% + {flowOffset(d)}px), -50%, 0) scale({d === 0 ? 1 : 0.74})"
            style:opacity={d === 0 ? 1 : Math.max(0.15, 0.75 - Math.abs(d) * 0.13)}
            onclick={() => (d === 0 ? onopen(i) : (index = i))}
          >
            <Cover path={r.cover_large ?? r.cover_small} title={r.title} focused={d === 0 && active} />
          </div>
        {/each}
        {#if roms.length}<div class="position">{index + 1} / {roms.length}</div>{/if}
      </div>
      {#if rom}
        <div class="caption">
          <GameInfo title={rom.title} detail={focus.detail?.id === rom.id ? focus.detail : null} lines={2} />
        </div>
      {/if}
    </div>
  {:else}
    {@const big = view === 'covers'}
    <div class="top" class:slim={big}>
      {#if rom}
        <GameInfo title={rom.title} detail={focus.detail?.id === rom.id ? focus.detail : null} lines={big ? 0 : 2} />
      {/if}
    </div>
    <div class="grid">
      {#key big}
        <VirtualGrid
          items={roms}
          bind:index
          bind:cols
          bind:rows
          tileWidth={big ? 210 : 150}
          tileHeight={big ? 280 : 200}
          gap={big ? 26 : 20}
          onactivate={onopen}
        >
          {#snippet tile(r: Rom, _i: number, focused: boolean)}
            <Cover path={big ? (r.cover_large ?? r.cover_small) : r.cover_small} title={r.title} focused={focused && active} />
          {/snippet}
        </VirtualGrid>
      {/key}
    </div>
  {/if}

  {#if letter}<div class="letter">{letter}</div>{/if}
</div>

<style>
  .browser {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .list,
  .panel,
  .top,
  .grid {
    position: relative;
    z-index: 1;
  }

  /* ---- list view ---- */
  .list {
    width: 42%;
    max-width: 540px;
    padding: 4px 0 4px 24px;
  }
  .row {
    height: 100%;
    display: flex;
    align-items: center;
    padding: 0 16px;
    margin-right: 12px;
    border-radius: 8px;
    gap: 12px;
    font-size: 17px;
    color: var(--text-2);
  }
  .row .t {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row .tag {
    flex: none;
    margin-left: auto;
    max-width: 45%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 13px;
    color: var(--muted);
  }
  .row .here {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
  }
  .row .t + .here {
    margin-left: auto;
  }
  .row.focused .here {
    background: #fff;
  }
  .row.focused .tag {
    color: rgb(255 255 255 / 0.8);
  }
  .row.focused {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .row.focused.dim {
    background: rgb(255 255 255 / 0.1);
  }
  .panel {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 22px;
    padding: 8px 40px 12px 28px;
  }
  .art {
    position: relative;
    height: 300px;
    flex: none;
  }
  .shot {
    position: absolute;
    left: 0;
    top: 0;
    width: calc(100% - 70px);
    height: 100%;
    border-radius: 12px;
    overflow: hidden;
    background: rgb(255 255 255 / 0.04);
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.45);
  }
  .shot img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    animation: fade 220ms ease-out;
  }
  .box {
    position: absolute;
    right: 0;
    bottom: -14px;
    width: 150px;
    height: 200px;
  }

  /* ---- grid and covers views ---- */
  .browser.view-grid,
  .browser.view-covers,
  .browser.view-carousel {
    flex-direction: column;
  }
  .top {
    height: 150px;
    flex: none;
    padding: 6px 44px 0;
    max-width: 900px;
  }
  .top.slim {
    height: 92px;
  }

  /* ---- carousel view ---- */
  .flow {
    position: relative;
    z-index: 1;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .strip {
    position: relative;
    flex: 1;
    min-height: 300px;
    overflow: hidden;
  }
  .card {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 300px;
    height: 400px;
    transition:
      transform 240ms cubic-bezier(0.2, 0.8, 0.2, 1),
      opacity 240ms;
    will-change: transform, opacity;
  }
  .caption {
    flex: none;
    position: relative;
    height: 132px;
    padding: 0 44px;
    max-width: 900px;
    width: 100%;
    margin: 0 auto;
    text-align: center;
  }
  .caption :global(.facts) {
    justify-content: center;
  }
  .position {
    position: absolute;
    z-index: 20;
    right: 44px;
    top: 4px;
    font-size: 13px;
    color: var(--muted);
  }
  .grid {
    flex: 1;
    min-height: 0;
    padding: 0 16px;
  }

  .letter {
    position: absolute;
    z-index: 3;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 140px;
    height: 140px;
    border-radius: 28px;
    background: rgb(0 0 0 / 0.72);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 78px;
    font-weight: 800;
    pointer-events: none;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
</style>
