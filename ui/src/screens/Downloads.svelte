<script lang="ts">
  // Running and waiting downloads. Downloads run one at a time and continue after a
  // restart; X removes the focused one (a stopped download resumes where it was).
  import { onDestroy } from 'svelte'
  import Footer from '../components/Footer.svelte'
  import Header from '../components/Header.svelte'
  import { games, type DownloadItem } from '../lib/api'
  import { onAction, type Action } from '../lib/input'
  import { progressText } from '../lib/play.svelte'
  import { toast } from '../lib/toast.svelte'

  let items = $state<DownloadItem[]>([])
  let index = $state(0)

  async function load() {
    try {
      items = await games.downloads()
      index = Math.min(index, Math.max(items.length - 1, 0))
    } catch (e) {
      toast(String(e), 'error')
    }
  }
  load()
  const timer = setInterval(load, 1000)

  const off = onAction(({ action }) => {
    switch (action) {
      case 'up':
        index = Math.max(index - 1, 0)
        return true
      case 'down':
        index = Math.min(index + 1, Math.max(items.length - 1, 0))
        return true
      case 'x': {
        const it = items[index]
        if (it) games.cancel(it.rom_id).then(load)
        return true
      }
    }
    return false
  })
  onDestroy(() => {
    off()
    clearInterval(timer)
  })

  const hints = $derived<[Action, string][]>(
    items.length ? [['x', items[index]?.state === 'active' ? 'Stop' : 'Remove'], ['b', 'Back']] : [['b', 'Back']],
  )
</script>

<Header title="Downloads" subtitle={items.length ? `${items.length} in the queue` : ''} />
<main>
  {#if items.length === 0}
    <p class="empty">Nothing downloading. Press A on a game's page to download it.</p>
  {:else}
    {#each items as it, i (it.rom_id)}
      <div class="row" class:focused={i === index}>
        <div class="top">
          <span class="title">{it.title}</span>
          <span class="platform">{it.platform}</span>
        </div>
        {#if it.state === 'active'}
          <div class="bar"><div class="fill" style:transform="scaleX({it.total ? it.done / it.total : 0})"></div></div>
          <div class="sub">{progressText(it)}</div>
        {:else}
          <div class="sub">Waiting · #{i} in line</div>
        {/if}
      </div>
    {/each}
  {/if}
</main>
<Footer {hints} />

<style>
  main {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 8px 36px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .empty {
    margin: auto;
    color: var(--muted);
    font-size: 18px;
  }
  .row {
    padding: 14px 18px;
    border-radius: 10px;
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row.focused {
    background: var(--surface-2);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .top {
    display: flex;
    justify-content: space-between;
    gap: 16px;
  }
  .title {
    font-size: 17px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .platform,
  .sub {
    color: var(--muted);
    font-size: 14px;
    white-space: nowrap;
  }
  .bar {
    height: 8px;
    border-radius: 4px;
    background: rgb(255 255 255 / 0.12);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transform-origin: left;
    transition: transform 400ms linear;
  }
</style>
