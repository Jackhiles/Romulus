<script lang="ts">
  import { onDestroy, untrack } from 'svelte'
  import Cover from '../components/Cover.svelte'
  import Footer from '../components/Footer.svelte'
  import Header from '../components/Header.svelte'
  import VirtualGrid from '../components/VirtualGrid.svelte'
  import { api, type Rom, type Sort } from '../lib/api'
  import { jumpLetter, letterOf, page, step } from '../lib/grid'
  import { onAction } from '../lib/input'
  import { library } from '../lib/library.svelte'
  import { push, type Screen } from '../lib/nav.svelte'

  let { platformId, platformName, entry }: { platformId: number; platformName: string; entry: Screen } = $props()

  let roms = $state<Rom[]>([])
  // svelte-ignore state_referenced_locally
  let index = $state(entry.index ?? 0)
  let cols = $state(1)
  let rows = $state(1)
  // svelte-ignore state_referenced_locally
  let sort = $state<Sort>(entry.sort ?? 'title')
  $effect(() => {
    entry.index = index
    entry.sort = sort
  })
  let letter = $state<string | null>(null)
  let letterTimer: ReturnType<typeof setTimeout> | undefined

  $effect(() => {
    void library.revision
    const order = sort
    // Keep focus on the same game across reloads (read without tracking, or this
    // effect would re-run every time it loads).
    const keep = untrack(() => roms[index]?.id)
    api.roms(platformId, order).then((r) => {
      roms = r
      const i = keep ? r.findIndex((x) => x.id === keep) : -1
      index = i >= 0 ? i : Math.min(index, Math.max(r.length - 1, 0))
    })
  })

  function showLetter() {
    if (sort !== 'title') return
    letter = letterOf(roms[index]?.title ?? '')
    clearTimeout(letterTimer)
    letterTimer = setTimeout(() => (letter = null), 700)
  }

  function open(i: number) {
    const r = roms[i]
    if (r) push({ name: 'detail', romId: r.id, siblings: roms.map((x) => x.id) })
  }

  const off = onAction(({ action }) => {
    const n = roms.length
    switch (action) {
      case 'up': case 'down': case 'left': case 'right':
        index = step(index, action, cols, n)
        return true
      case 'lb':
        index = page(index, -1, cols, rows, n)
        return true
      case 'rb':
        index = page(index, 1, cols, rows, n)
        return true
      case 'lt':
      case 'rt':
        if (sort === 'title') {
          index = jumpLetter(roms.map((r) => r.title), index, action === 'rt' ? 1 : -1)
          showLetter()
        } else {
          index = page(index, action === 'rt' ? 3 : -3, cols, rows, n)
        }
        return true
      case 'a':
        open(index)
        return true
      case 'x':
        sort = sort === 'title' ? 'newest' : 'title'
        index = 0
        return true
      case 'y':
        push({ name: 'search' })
        return true
      case 'start':
        push({ name: 'settings' })
        return true
    }
    return false
  })
  onDestroy(() => {
    off()
    clearTimeout(letterTimer)
  })

  const focused = $derived(roms[index])
</script>

<Header title={platformName} subtitle={`${roms.length} games · ${sort === 'title' ? 'A–Z' : 'Newest'}`} />
<main>
  <VirtualGrid items={roms} bind:index bind:cols bind:rows tileWidth={172} tileHeight={230} gap={22} onactivate={open}>
    {#snippet tile(r: Rom, _i: number, isFocused: boolean)}
      <Cover path={r.cover_small} title={r.title} focused={isFocused} />
    {/snippet}
  </VirtualGrid>
  {#if letter}<div class="letter">{letter}</div>{/if}
</main>
<div class="focus-title">{focused?.title ?? ''}</div>
<Footer
  hints={[
    ['a', 'Details'],
    ['x', sort === 'title' ? 'Sort: newest' : 'Sort: A–Z'],
    ['y', 'Search'],
    ['lt', sort === 'title' ? 'Letter' : 'Jump'],
    ['b', 'Back'],
  ]}
/>

<style>
  main {
    flex: 1;
    min-height: 0;
    padding: 0 20px;
    position: relative;
  }
  .focus-title {
    padding: 0 32px;
    height: 26px;
    font-size: 18px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .letter {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 140px;
    height: 140px;
    border-radius: 24px;
    background: rgb(0 0 0 / 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 80px;
    font-weight: 800;
    pointer-events: none;
  }
</style>
