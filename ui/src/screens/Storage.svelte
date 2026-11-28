<script lang="ts">
  // What Romulus downloaded onto this Deck and how much room is left. Games are listed
  // largest first (Y: least recently played first); X removes the focused game's files
  // (never its saves), A opens its page.
  import { onDestroy } from 'svelte'
  import Confirm from '../components/Confirm.svelte'
  import Footer from '../components/Footer.svelte'
  import Header from '../components/Header.svelte'
  import VirtualList from '../components/VirtualList.svelte'
  import { games, type InstalledGame, type Storage } from '../lib/api'
  import { ago, size } from '../lib/format'
  import { onAction, type Action } from '../lib/input'
  import { push } from '../lib/nav.svelte'
  import { removed } from '../lib/play.svelte'
  import { usageBySystem } from '../lib/playStatus'
  import { toast } from '../lib/toast.svelte'

  let data = $state<Storage | null>(null)
  let index = $state(0)
  let byAge = $state(false)
  let confirm = $state<InstalledGame | null>(null)
  let rows = $state(1)

  async function load() {
    try {
      data = await games.storage()
    } catch (e) {
      toast(String(e), 'error', 6000)
    }
  }
  load()

  const list = $derived.by(() => {
    const g = [...(data?.games ?? [])]
    if (byAge) g.sort((a, b) => (a.last_played ?? 0) - (b.last_played ?? 0) || b.disk_bytes - a.disk_bytes)
    return g
  })
  const usage = $derived(usageBySystem(data?.games ?? []))
  const used = $derived((data?.games ?? []).reduce((s, g) => s + g.disk_bytes, 0))
  $effect(() => {
    if (index >= list.length) index = Math.max(list.length - 1, 0)
  })

  async function remove(g: InstalledGame) {
    confirm = null
    try {
      await games.remove(g.id)
      removed(g.id)
      toast(`${g.title} removed (${size(g.disk_bytes)} freed). Its saves were not touched.`)
      load()
    } catch (e) {
      toast(String(e), 'error', 6000)
    }
  }

  const off = onAction(({ action }) => {
    if (confirm) return false
    const g = list[index]
    const last = Math.max(list.length - 1, 0)
    switch (action) {
      case 'up':
      case 'down':
        index = Math.min(Math.max(index + (action === 'down' ? 1 : -1), 0), last)
        return true
      case 'lb':
      case 'rb':
        index = Math.min(Math.max(index + (action === 'rb' ? rows : -rows), 0), last)
        return true
      case 'x':
        if (g) confirm = g
        return true
      case 'a':
        if (g) push({ name: 'detail', romId: g.id, siblings: list.map((x) => x.id) })
        return true
      case 'y':
        byAge = !byAge
        index = 0
        return true
    }
    return false
  })
  onDestroy(off)

  const hints = $derived<[Action, string][]>(
    list.length
      ? [
          ['a', 'Details'],
          ['x', 'Remove'],
          ['y', byAge ? 'Largest first' : 'Least played first'],
          ['b', 'Back'],
        ]
      : [['b', 'Back']],
  )
  const pct = (n: number) => (data && data.total_bytes ? Math.min(100, (n / data.total_bytes) * 100) : 0)
</script>

<Header
  title="Storage"
  subtitle={data ? `${size(data.free_bytes)} free of ${size(data.total_bytes)} · Romulus games use ${size(used)}` : ''}
/>
<main>
  {#if data}
    <div class="drive">
      <div class="bar">
        <div class="seg ours" style:width="{pct(used)}%"></div>
        <div class="seg other" style:width="{Math.max(0, pct(data.total_bytes - data.free_bytes - used))}%"></div>
      </div>
      <div class="legend">
        <span><i class="dot ours"></i>Games from Romulus</span>
        <span><i class="dot other"></i>Everything else</span>
        <span><i class="dot free"></i>Free</span>
      </div>
      {#if usage.length}
        <div class="systems">
          {#each usage.slice(0, 6) as u (u.platform)}
            <span class="sys">{u.platform || 'Other'} <b>{size(u.bytes)}</b> · {u.games}</span>
          {/each}
        </div>
      {/if}
    </div>
    {#if list.length === 0}
      <p class="empty">No games downloaded by Romulus on this Deck.</p>
    {:else}
      <div class="list">
        <VirtualList items={list} bind:index bind:rows onactivate={(i) => push({ name: 'detail', romId: list[i].id })}>
          {#snippet row(g: InstalledGame, _i: number, focused: boolean)}
            <div class="row" class:focused>
              <span class="t">{g.title}</span>
              <span class="p">{g.platform}</span>
              <span class="when">{g.last_played ? `played ${ago(g.last_played)}` : 'never played'}</span>
              <span class="s">{size(g.disk_bytes)}</span>
            </div>
          {/snippet}
        </VirtualList>
      </div>
    {/if}
  {/if}
</main>
<Footer {hints} />
{#if confirm}
  <Confirm
    title="Remove {confirm.title}?"
    message="Its game files ({size(confirm.disk_bytes)}) are deleted from this Deck. Saves and states stay, and you can download it again from RomM."
    confirm="Remove"
    onconfirm={() => confirm && remove(confirm)}
    oncancel={() => (confirm = null)}
  />
{/if}

<style>
  main {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 4px 36px 8px;
  }
  .drive {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .bar {
    height: 14px;
    border-radius: 7px;
    background: rgb(255 255 255 / 0.1);
    overflow: hidden;
    display: flex;
  }
  .seg.ours,
  .dot.ours {
    background: var(--accent);
  }
  .seg.other,
  .dot.other {
    background: rgb(255 255 255 / 0.35);
  }
  .dot.free {
    background: rgb(255 255 255 / 0.1);
  }
  .legend,
  .systems {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    font-size: 13px;
    color: var(--muted);
  }
  .dot {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 3px;
    margin-right: 6px;
    vertical-align: -1px;
  }
  .sys b {
    color: var(--text-2);
    font-weight: 600;
  }
  .list {
    flex: 1;
    min-height: 0;
  }
  .row {
    height: 100%;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 16px;
    border-radius: 8px;
    font-size: 16px;
    color: var(--text-2);
  }
  .row.focused {
    background: var(--accent);
    color: #fff;
  }
  .t {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .p,
  .when {
    flex: none;
    font-size: 13px;
    color: var(--muted);
    width: 170px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .when {
    width: 130px;
  }
  .row.focused .p,
  .row.focused .when {
    color: rgb(255 255 255 / 0.8);
  }
  .s {
    flex: none;
    width: 80px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .empty {
    margin: auto;
    color: var(--muted);
    font-size: 18px;
  }
</style>
