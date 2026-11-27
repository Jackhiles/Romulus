<script lang="ts">
  // A game's RetroAchievements: earned first, then the rest. From RomM's metadata and
  // the RetroAchievements progress RomM keeps for the account.
  import { onDestroy } from 'svelte'
  import Badge from '../components/Badge.svelte'
  import Footer from '../components/Footer.svelte'
  import Header from '../components/Header.svelte'
  import VirtualList from '../components/VirtualList.svelte'
  import { achievements, type Achievement, type Achievements } from '../lib/api'
  import { onAction } from '../lib/input'

  let { romId, title }: { romId: number; title: string } = $props()
  let data = $state<Achievements | null>(null)
  let index = $state(0)
  let rows = $state(1)

  // Mounted per stack entry, so the initial id is the only one.
  // svelte-ignore state_referenced_locally
  achievements(romId).then((a) => (data = a))

  const off = onAction(({ action }) => {
    const n = data?.list.length ?? 0
    switch (action) {
      case 'up':
      case 'down':
        index = Math.min(Math.max(index + (action === 'down' ? 1 : -1), 0), Math.max(n - 1, 0))
        return true
      case 'lb':
      case 'rb':
        index = Math.min(Math.max(index + (action === 'rb' ? rows : -rows), 0), Math.max(n - 1, 0))
        return true
    }
    return false
  })
  onDestroy(off)

  function when(t: string | null): string {
    if (!t) return ''
    const d = new Date(t.includes('T') ? t : `${t.replace(' ', 'T')}Z`)
    return isNaN(d.getTime()) ? t : d.toLocaleDateString([], { dateStyle: 'medium' })
  }
</script>

<Header
  {title}
  subtitle={data ? `${data.earned} of ${data.total} achievements · ${data.points_earned} of ${data.points_total} points` : ''}
/>
<main>
  {#if data}
    <VirtualList items={data.list} bind:index bind:rows rowHeight={76}>
      {#snippet row(a: Achievement, _i: number, focused: boolean)}
        <div class="ach" class:focused class:locked={!a.earned}>
          <Badge path={a.badge} size={52} />
          <div class="text">
            <div class="t">{a.title}</div>
            <div class="d">{a.description}</div>
          </div>
          <div class="side">
            <div class="pts">{a.points} pts</div>
            {#if a.earned}<div class="date">{when(a.earned_at)}</div>{/if}
          </div>
        </div>
      {/snippet}
    </VirtualList>
  {/if}
</main>
<Footer hints={[['lb', 'Page'], ['b', 'Back']]} />

<style>
  main {
    flex: 1;
    min-height: 0;
    padding: 0 24px;
  }
  .ach {
    height: 100%;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 16px;
    margin: 0 8px;
    border-radius: 10px;
  }
  .ach.focused {
    background: var(--surface-2);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .ach.locked .t,
  .ach.locked .d {
    opacity: 0.6;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .t {
    font-size: 17px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .d {
    color: var(--muted);
    font-size: 14px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .side {
    text-align: right;
    flex: none;
  }
  .pts {
    color: var(--accent);
    font-weight: 700;
    font-size: 14px;
  }
  .date {
    color: var(--muted);
    font-size: 13px;
  }
</style>
