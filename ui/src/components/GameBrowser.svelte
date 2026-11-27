<script lang="ts">
  // A system's games (or search results) as a list with a detail panel, or as a cover
  // grid with the focused game across the top. Handles moving the focus; the screen
  // using it handles A, X, Y, Select and (in list view) left/right.
  import { onDestroy } from 'svelte'
  import { music, type GamesView, type Rom } from '../lib/api'
  import { FocusedDetail } from '../lib/details.svelte'
  import { fileTags } from '../lib/format'
  import { device } from '../lib/play.svelte'
  import { jumpLetter, letterOf, page, step } from '../lib/grid'
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
  const tagOf = (r: Rom) => (tag ? tag(r) : repeated.has(r.title) ? fileTags(r.fs_name) : '')

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

  const off = onAction(({ action }) => {
    if (!active) return false
    const n = roms.length
    const perPage = view === 'list' ? rows : cols * rows
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
          index = Math.min(Math.max(index + (action === 'rt' ? 3 : -3) * perPage, 0), Math.max(n - 1, 0))
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
  {:else}
    <div class="top">
      {#if rom}
        <GameInfo title={rom.title} detail={focus.detail?.id === rom.id ? focus.detail : null} lines={2} />
      {/if}
    </div>
    <div class="grid">
      <VirtualGrid items={roms} bind:index bind:cols bind:rows tileWidth={150} tileHeight={200} gap={20} onactivate={onopen}>
        {#snippet tile(r: Rom, _i: number, focused: boolean)}
          <Cover path={r.cover_small} title={r.title} focused={focused && active} />
        {/snippet}
      </VirtualGrid>
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

  /* ---- grid view ---- */
  .browser.view-grid {
    flex-direction: column;
  }
  .top {
    height: 150px;
    flex: none;
    padding: 6px 44px 0;
    max-width: 900px;
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
