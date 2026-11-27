<script lang="ts">
  // Search the cached library as you type. Down moves from the field into the results;
  // Up on the first result goes back to the field.
  import { onDestroy, onMount } from 'svelte'
  import Footer from '../components/Footer.svelte'
  import GameBrowser from '../components/GameBrowser.svelte'
  import { api, type Rom } from '../lib/api'
  import { onAction, type Action } from '../lib/input'
  import { push, type Screen } from '../lib/nav.svelte'

  let { entry = $bindable() }: { entry: Screen } = $props()

  // svelte-ignore state_referenced_locally
  let query = $state(entry.query ?? '')
  let results = $state<Rom[]>([])
  // Results mix systems, so each row names its system.
  let systems = $state<Record<number, string>>({})
  api.platforms().then((ps) => (systems = Object.fromEntries(ps.map((p) => [p.id, p.slug.toUpperCase()]))))
  // svelte-ignore state_referenced_locally
  let index = $state(entry.index ?? 0)
  // svelte-ignore state_referenced_locally
  let inField = $state(entry.index === undefined)
  let input: HTMLInputElement | undefined = $state()
  let timer: ReturnType<typeof setTimeout> | undefined
  let seq = 0
  // svelte-ignore state_referenced_locally
  let restoring = entry.index !== undefined

  $effect(() => {
    entry.query = query
    entry.index = inField ? undefined : index
  })

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

  function toResults() {
    if (results.length === 0) return
    inField = false
    input?.blur()
  }

  const off = onAction(({ action }) => {
    if (inField) {
      switch (action) {
        case 'down':
          toResults()
          return true
        case 'a':
          if (document.activeElement !== input) {
            input?.focus()
            api.openKeyboard()
          } else toResults()
          return true
        case 'x':
          query = ''
          return true
        case 'b':
          return false
      }
      return true
    }
    switch (action) {
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

  const hints = $derived<[Action, string][]>(
    inField
      ? [['a', 'Keyboard'], ['down', 'Results'], ['x', 'Clear'], ['b', 'Back']]
      : [['a', 'Details'], ['y', 'Edit search'], ['b', 'Back']],
  )
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
{#if query.trim() && results.length === 0}
  <div class="empty">No matches</div>
{:else}
  <GameBrowser roms={results} bind:index view="list" active={!inField} letters={false} onopen={open} ontop={toField} tag={(r) => systems[r.platform_id] ?? ''} />
{/if}
<Footer {hints} />

<style>
  .bar {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 20px 36px 14px;
  }
  input {
    flex: 1;
    font-size: 20px;
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
  .empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--muted);
    font-size: 20px;
  }
</style>
