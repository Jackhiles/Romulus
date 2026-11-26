<script lang="ts">
  import { onDestroy } from 'svelte'
  import Footer from '../components/Footer.svelte'
  import Header from '../components/Header.svelte'
  import VirtualGrid from '../components/VirtualGrid.svelte'
  import { api, type Platform } from '../lib/api'
  import { page, step } from '../lib/grid'
  import { onAction } from '../lib/input'
  import { library, refresh } from '../lib/library.svelte'
  import { push, type Screen } from '../lib/nav.svelte'

  let { entry }: { entry: Screen } = $props()

  let platforms = $state<Platform[]>([])
  // svelte-ignore state_referenced_locally
  let index = $state(entry.index ?? 0)
  $effect(() => {
    entry.index = index
  })
  let cols = $state(1)
  let rows = $state(1)
  let loaded = $state(false)

  $effect(() => {
    void library.revision
    api.platforms().then((p) => {
      platforms = p
      index = Math.min(index, Math.max(p.length - 1, 0))
      loaded = true
    })
  })

  function open(i: number) {
    const p = platforms[i]
    if (p) push({ name: 'games', platformId: p.id, platformName: p.name })
  }

  const off = onAction(({ action }) => {
    const n = platforms.length
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
      case 'a':
        open(index)
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

  const total = $derived(platforms.reduce((s, p) => s + p.rom_count, 0))
</script>

<Header title="Library" subtitle={total ? `${total} games` : ''} />
<main>
  {#if loaded && platforms.length === 0}
    <div class="empty">
      {#if library.refreshing}
        <p>Loading your library…</p>
      {:else}
        <p>No games yet. Press <strong>X</strong> to sync with RomM.</p>
      {/if}
    </div>
  {:else}
    <VirtualGrid items={platforms} bind:index bind:cols bind:rows tileWidth={268} tileHeight={150} gap={24} onactivate={open}>
      {#snippet tile(p: Platform, _i: number, focused: boolean)}
        <div class="card" class:focused>
          <div class="abbr">{p.slug.toUpperCase()}</div>
          <div class="name">{p.name}</div>
          <div class="count">{p.rom_count} {p.rom_count === 1 ? 'game' : 'games'}</div>
        </div>
      {/snippet}
    </VirtualGrid>
  {/if}
</main>
<Footer hints={[['a', 'Open'], ['y', 'Search'], ['x', 'Sync'], ['start', 'Settings']]} />

<style>
  main {
    flex: 1;
    min-height: 0;
    padding: 0 20px;
  }
  .card {
    width: 100%;
    height: 100%;
    border-radius: var(--radius);
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    gap: 4px;
    background: linear-gradient(150deg, var(--surface-2), var(--surface));
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.35);
    transition: transform 120ms ease-out, box-shadow 120ms ease-out;
    position: relative;
    overflow: hidden;
  }
  .focused {
    transform: scale(1.05);
    box-shadow: 0 0 0 3px var(--accent), 0 10px 28px rgb(0 0 0 / 0.55);
  }
  .abbr {
    position: absolute;
    top: 8px;
    right: 14px;
    font-size: 34px;
    font-weight: 800;
    color: rgb(255 255 255 / 0.07);
  }
  .name {
    font-size: 19px;
    font-weight: 700;
    line-height: 1.2;
  }
  .count {
    color: var(--muted);
    font-size: 14px;
  }
  .empty {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
    color: var(--muted);
  }
</style>
