<script lang="ts">
  // Search the cached library as you type. Down moves from the field into the results.
  import { onDestroy, onMount } from 'svelte'
  import Cover from '../components/Cover.svelte'
  import Footer from '../components/Footer.svelte'
  import VirtualGrid from '../components/VirtualGrid.svelte'
  import { api, type Rom } from '../lib/api'
  import { page, step } from '../lib/grid'
  import { onAction } from '../lib/input'
  import { push, type Screen } from '../lib/nav.svelte'

  let { entry }: { entry: Screen } = $props()

  // svelte-ignore state_referenced_locally
  let query = $state(entry.query ?? '')
  let results = $state<Rom[]>([])
  // svelte-ignore state_referenced_locally
  let index = $state(entry.index ?? 0)
  let cols = $state(1)
  let rows = $state(1)
  // svelte-ignore state_referenced_locally
  let inField = $state(entry.index === undefined)
  $effect(() => {
    entry.query = query
    entry.index = inField ? undefined : index
  })
  let input: HTMLInputElement | undefined = $state()
  let timer: ReturnType<typeof setTimeout> | undefined
  let seq = 0
  // svelte-ignore state_referenced_locally
  let restoring = entry.index !== undefined

  onMount(() => {
    if (inField) {
      input?.focus()
      api.openKeyboard()
    }
  })

  $effect(() => {
    const q = query
    clearTimeout(timer)
    timer = setTimeout(() => {
      const mine = ++seq
      api.search(q).then((r) => {
        if (mine !== seq) return
        results = r
        index = restoring ? Math.min(index, Math.max(r.length - 1, 0)) : 0
        restoring = false
      })
    }, 120)
  })

  function open(i: number) {
    const r = results[i]
    if (r) push({ name: 'detail', romId: r.id, siblings: results.map((x) => x.id) })
  }

  function toField() {
    inField = true
    input?.focus()
  }

  const off = onAction(({ action }) => {
    const n = results.length
    if (inField) {
      if (action === 'down' && n > 0) {
        inField = false
        input?.blur()
        return true
      }
      if (action === 'a') {
        if (document.activeElement !== input) {
          input?.focus()
          api.openKeyboard()
        } else if (n > 0) {
          inField = false
          input?.blur()
        }
        return true
      }
      if (action === 'x') {
        query = ''
        return true
      }
      return action !== 'b'
    }
    switch (action) {
      case 'up':
        if (index < cols) toField()
        else index = step(index, 'up', cols, n)
        return true
      case 'down': case 'left': case 'right':
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
        toField()
        return true
    }
    return false
  })
  onDestroy(() => {
    off()
    clearTimeout(timer)
  })
</script>

<div class="bar">
  <input
    bind:this={input}
    bind:value={query}
    type="search"
    placeholder="Search your library"
    spellcheck="false"
    autocapitalize="off"
    class:active={inField}
    onfocus={() => (inField = true)}
  />
  <span class="count">{query.trim() ? `${results.length}${results.length === 200 ? '+' : ''} found` : ''}</span>
</div>
<main>
  {#if query.trim() && results.length === 0}
    <div class="empty">No matches</div>
  {:else}
    <VirtualGrid items={results} bind:index bind:cols bind:rows tileWidth={172} tileHeight={230} gap={22} onactivate={open}>
      {#snippet tile(r: Rom, _i: number, focused: boolean)}
        <Cover path={r.cover_small} title={r.title} focused={focused && !inField} />
      {/snippet}
    </VirtualGrid>
  {/if}
</main>
<div class="focus-title">{!inField ? (results[index]?.title ?? '') : ''}</div>
<Footer
  hints={inField
    ? [['a', 'Keyboard'], ['down', 'Results'], ['x', 'Clear'], ['b', 'Back']]
    : [['a', 'Details'], ['y', 'Edit search'], ['b', 'Back']]}
/>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 24px 32px 12px;
  }
  input {
    flex: 1;
    font-size: 22px;
    padding: 12px 18px;
  }
  input.active {
    box-shadow: 0 0 0 3px var(--accent);
  }
  .count {
    color: var(--muted);
    min-width: 90px;
    text-align: right;
  }
  main {
    flex: 1;
    min-height: 0;
    padding: 0 20px;
  }
  .empty {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--muted);
    font-size: 20px;
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
</style>
