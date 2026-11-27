<script lang="ts">
  // Saves Romulus replaced, per game. A restores the focused version (after a
  // confirmation); the current save is backed up first, so restoring can be undone.
  import { onDestroy } from 'svelte'
  import Confirm from '../components/Confirm.svelte'
  import Footer from '../components/Footer.svelte'
  import Header from '../components/Header.svelte'
  import { backups, type GameBackups } from '../lib/api'
  import { describeBackup } from '../lib/backups'
  import { onAction, type Action } from '../lib/input'
  import { device } from '../lib/play.svelte'
  import { toast } from '../lib/toast.svelte'

  let games = $state<GameBackups[] | null>(null)
  let gi = $state(0)
  let vi = $state(0)
  let inVersions = $state(false)
  let confirming = $state(false)

  async function load() {
    games = await backups.list().catch((e) => {
      toast(String(e), 'error')
      return []
    })
    gi = Math.min(gi, Math.max((games?.length ?? 1) - 1, 0))
  }
  load()

  const game = $derived(games?.[gi])

  async function restore() {
    confirming = false
    const g = game
    const v = g?.versions[vi]
    if (!g || !v) return
    try {
      await backups.restore(g.rom_id, v.label)
      toast(`Restored. It will upload to RomM at the next sync.`)
      device.revision++
      await load()
    } catch (e) {
      toast(String(e), 'error', 7000)
    }
  }

  const off = onAction(({ action }) => {
    const n = inVersions ? (game?.versions.length ?? 0) : (games?.length ?? 0)
    switch (action) {
      case 'up':
      case 'down': {
        const d = action === 'down' ? 1 : -1
        if (inVersions) vi = Math.min(Math.max(vi + d, 0), Math.max(n - 1, 0))
        else {
          gi = Math.min(Math.max(gi + d, 0), Math.max(n - 1, 0))
          vi = 0
        }
        return true
      }
      case 'right':
        if (game) inVersions = true
        return true
      case 'left':
        inVersions = false
        return true
      case 'a':
        if (!inVersions) inVersions = !!game
        else if (game?.versions[vi]) confirming = true
        return true
      case 'b':
        if (inVersions) {
          inVersions = false
          return true
        }
        return false
    }
    return false
  })
  onDestroy(off)

  const hints = $derived<[Action, string][]>(
    inVersions ? [['a', 'Restore'], ['b', 'Games']] : game ? [['a', 'Versions'], ['b', 'Back']] : [['b', 'Back']],
  )
</script>

<Header title="Save backups" subtitle="Saves Romulus replaced. Nothing here is ever deleted." />
<main>
  {#if games && games.length === 0}
    <p class="empty">No backups yet. Romulus makes one whenever it replaces a save.</p>
  {:else if games}
    <div class="list">
      {#each games as g, i (g.rom_id)}
        <div class="row" class:focused={i === gi && !inVersions} class:current={i === gi}>
          <span>{g.title}</span><span class="n">{g.versions.length}</span>
        </div>
      {/each}
    </div>
    {#if game}
      <div class="versions">
        {#each game.versions as v, i (v.label)}
          {@const d = describeBackup(v.label)}
          <div class="ver" class:focused={inVersions && i === vi}>
            <div class="when">{d.when}</div>
            <div class="why">{d.why}</div>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</main>
<Footer {hints} />
{#if confirming && game}
  <Confirm
    title="Restore this save?"
    message={`${game.title} goes back to the save from ${describeBackup(game.versions[vi]?.label ?? '').when}. The save it has now is backed up first.`}
    confirm="Restore"
    onconfirm={restore}
    oncancel={() => (confirming = false)}
  />
{/if}

<style>
  main {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 28px;
    padding: 8px 36px;
    overflow: hidden;
  }
  .empty {
    margin: auto;
    color: var(--muted);
    font-size: 18px;
  }
  .list {
    width: 42%;
    max-width: 480px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 16px;
    border-radius: 8px;
    color: var(--text-2);
    font-size: 17px;
  }
  .row.current {
    background: rgb(255 255 255 / 0.06);
  }
  .row.focused {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .n {
    color: inherit;
    opacity: 0.7;
    font-size: 14px;
  }
  .versions {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ver {
    padding: 10px 14px;
    border-radius: 8px;
    background: var(--surface);
  }
  .ver.focused {
    background: var(--surface-2);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .when {
    font-weight: 600;
  }
  .why {
    color: var(--muted);
    font-size: 14px;
  }
</style>
