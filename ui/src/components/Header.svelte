<script lang="ts">
  // Screen title on the left, library sync state on the right.
  import { ago } from '../lib/format'
  import { library } from '../lib/library.svelte'

  let { title, subtitle = '' }: { title: string; subtitle?: string } = $props()

  // Re-render "x min ago" every 30 s.
  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 30_000)
    return () => clearInterval(t)
  })
</script>

<header>
  <div class="titles">
    <h1>{title}</h1>
    {#if subtitle}<span class="sub">{subtitle}</span>{/if}
  </div>
  <div class="sync" class:busy={library.refreshing}>
    {#if library.refreshing}
      <span class="spinner"></span>
      <span>{library.progress || 'Syncing…'}</span>
    {:else}
      <span>Last synced {ago(library.lastSync, now)}</span>
    {/if}
  </div>
</header>

<style>
  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding: 22px 32px 8px;
  }
  .titles {
    display: flex;
    align-items: baseline;
    gap: 14px;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    color: var(--muted);
    font-size: 16px;
  }
  .sync {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: 14px;
    white-space: nowrap;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--muted);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
