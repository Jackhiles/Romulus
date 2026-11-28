<script lang="ts" module>
  export interface MenuRow {
    label: string
    value?: string
    /** A: do it (and close the menu unless `stay`). */
    run?: () => void
    /** Left/right (and A): change the value. */
    cycle?: (dir: 1 | -1) => void
    stay?: boolean
  }
</script>

<script lang="ts">
  // A list of actions on top of the screen (a game's menu). Up/down move, A runs the row
  // or changes its value, left/right change values, B closes. Nothing else gets through
  // while it is open.
  import { onDestroy } from 'svelte'
  import { onAction } from '../lib/input'

  let { title, rows, onclose }: { title: string; rows: MenuRow[]; onclose: () => void } = $props()
  let index = $state(0)

  const off = onAction(({ action }) => {
    const row = rows[index]
    switch (action) {
      case 'up':
        index = (index - 1 + rows.length) % rows.length
        break
      case 'down':
        index = (index + 1) % rows.length
        break
      case 'left':
      case 'right':
        row?.cycle?.(action === 'right' ? 1 : -1)
        break
      case 'a':
        if (row?.run) {
          row.run()
          if (!row.stay) onclose()
        } else row?.cycle?.(1)
        break
      case 'b':
      case 'start':
        onclose()
        break
    }
    return true
  })
  onDestroy(off)
</script>

<div class="scrim">
  <div class="box" role="menu" aria-label={title}>
    <h2>{title}</h2>
    {#each rows as r, i (r.label)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        class="row"
        class:focused={i === index}
        role="menuitem"
        tabindex="-1"
        onclick={() => {
          index = i
          if (r.run) {
            r.run()
            if (!r.stay) onclose()
          } else r.cycle?.(1)
        }}
      >
        <span class="label">{r.label}</span>
        {#if r.value !== undefined}
          <span class="value">{#if r.cycle}<span class="arrow">‹</span>{/if}{r.value}{#if r.cycle}<span class="arrow">›</span>{/if}</span>
        {/if}
      </div>
    {/each}
    <div class="hint"><span class="btn">A</span> Select · <span class="btn">←→</span> Change · <span class="btn">B</span> Close</div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: rgb(0 0 0 / 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    animation: fade 140ms ease-out;
  }
  .box {
    width: min(560px, 92vw);
    padding: 24px 22px 18px;
    border-radius: 18px;
    background: var(--surface-2);
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.6);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  h2 {
    margin: 0 8px 10px;
    font-size: 20px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 14px;
    border-radius: 10px;
    font-size: 17px;
    color: var(--text-2);
  }
  .row.focused {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .value {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-weight: 500;
  }
  .row.focused .value {
    color: #fff;
  }
  .arrow {
    opacity: 0.6;
  }
  .hint {
    margin: 12px 8px 0;
    font-size: 13px;
    color: var(--muted);
  }
  .btn {
    font-weight: 700;
    color: var(--text-2);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
</style>
