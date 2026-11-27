<script lang="ts">
  // Both the Deck and RomM have a newer save than the last sync: the player picks. The
  // one not picked is kept as a backup either way.
  import { onDestroy } from 'svelte'
  import type { SaveChoice, SaveConflict } from '../lib/api'
  import { onAction } from '../lib/input'

  let {
    title,
    info,
    onchoose,
    oncancel,
  }: { title: string; info: SaveConflict; onchoose: (c: SaveChoice) => void; oncancel: () => void } = $props()

  function when(t: string | null): string {
    if (!t) return 'unknown time'
    // RomM sends UTC without a zone.
    const d = new Date(/[zZ]|[+-]\d\d:?\d\d$/.test(t) ? t : `${t}Z`)
    return isNaN(d.getTime()) ? t : d.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })
  }

  const off = onAction(({ action }) => {
    if (action === 'a') onchoose('use_server')
    else if (action === 'x') onchoose('keep_local')
    else if (action === 'b') oncancel()
    return true
  })
  onDestroy(off)
</script>

<div class="scrim">
  <div class="box" role="alertdialog" aria-labelledby="sc-title">
    <h2 id="sc-title">Which save should {title} use?</h2>
    <p>Both this Deck and RomM have saved since they last matched.</p>
    <div class="sides">
      <div class="side">
        <div class="k">RomM</div>
        <div class="v">{when(info.server_updated)}</div>
        <div class="d">{info.server_device ? 'from another device' : 'uploaded save'}</div>
      </div>
      <div class="side">
        <div class="k">This Deck</div>
        <div class="v">{when(info.local_modified)}</div>
        <div class="d">last changed here</div>
      </div>
    </div>
    <p class="small">The save you don't pick is backed up, not deleted.</p>
    <div class="buttons">
      <button tabindex="-1" class="primary-choice" onclick={() => onchoose('use_server')}><span class="btn a">A</span>Use RomM's</button>
      <button tabindex="-1" onclick={() => onchoose('keep_local')}><span class="btn x">X</span>Keep this Deck's</button>
      <button tabindex="-1" onclick={oncancel}><span class="btn b">B</span>Cancel</button>
    </div>
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
    width: min(620px, 92vw);
    padding: 28px 30px 24px;
    border-radius: 18px;
    background: var(--surface-2);
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.6);
  }
  h2 {
    margin: 0 0 8px;
    font-size: 22px;
  }
  p {
    margin: 0 0 16px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .small {
    font-size: 14px;
    color: var(--muted);
  }
  .sides {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    margin-bottom: 16px;
  }
  .side {
    padding: 14px 16px;
    border-radius: 12px;
    background: rgb(255 255 255 / 0.05);
  }
  .k {
    font-size: 12px;
    font-weight: 800;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--accent);
  }
  .v {
    margin-top: 6px;
    font-size: 17px;
    font-weight: 600;
  }
  .d {
    color: var(--muted);
    font-size: 13px;
  }
  .buttons {
    display: flex;
    gap: 12px;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
  button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border: 0;
    border-radius: 10px;
    font: inherit;
    font-weight: 600;
    color: var(--text);
    background: rgb(255 255 255 / 0.08);
    cursor: pointer;
  }
  .primary-choice {
    background: var(--accent);
    color: #fff;
  }
  .btn {
    width: 22px;
    height: 22px;
    border-radius: 11px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 800;
    background: rgb(0 0 0 / 0.3);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
</style>
