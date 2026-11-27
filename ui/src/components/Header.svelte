<script lang="ts">
  // Top bar: screen title on the left; sync state and the time on the right.
  import { ago } from '../lib/format'
  import { library } from '../lib/library.svelte'
  import { device, downloadCount } from '../lib/play.svelte'

  let { title, subtitle = '' }: { title: string; subtitle?: string } = $props()

  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 15_000)
    return () => clearInterval(t)
  })
  const clock = $derived(new Date(now).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }))
</script>

<header>
  <div class="titles">
    <h1>{title}</h1>
    {#if subtitle}<span class="sub">{subtitle}</span>{/if}
  </div>
  <div class="right">
    <span class="sync" class:busy={library.refreshing}>
      {#if library.refreshing}
        <span class="spinner"></span>{library.progress || 'Syncing…'}
      {:else}
        Synced {ago(library.lastSync, now)}
      {/if}
    </span>
    {#if downloadCount()}
      {@const active = Object.values(device.downloads).find((d) => !d.queued && d.total)}
      <span class="dl">⬇ {downloadCount()}{active ? ` · ${Math.floor((active.done / active.total) * 100)}%` : ''}</span>
    {/if}
    <span class="clock">{clock}</span>
  </div>
</header>

<style>
  header {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    height: 64px;
    padding: 0 36px;
    flex: none;
  }
  .titles {
    display: flex;
    align-items: baseline;
    gap: 14px;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    color: var(--muted);
    font-size: 14px;
    white-space: nowrap;
  }
  .right {
    display: flex;
    align-items: center;
    gap: 22px;
    color: var(--muted);
    font-size: 14px;
    white-space: nowrap;
  }
  .sync {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dl {
    color: var(--accent);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .clock {
    color: var(--text);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .spinner {
    width: 11px;
    height: 11px;
    border-radius: 50%;
    border: 2px solid var(--muted);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
