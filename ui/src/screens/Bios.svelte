<script lang="ts">
  // BIOS files from RomM, per system: what is on the Deck, and A to get what's missing.
  import { onDestroy } from 'svelte'
  import Footer from '../components/Footer.svelte'
  import Header from '../components/Header.svelte'
  import { bios, type BiosState, type BiosSystem } from '../lib/api'
  import { size } from '../lib/format'
  import { onAction, type Action } from '../lib/input'
  import { toast } from '../lib/toast.svelte'

  let systems = $state<BiosSystem[] | null>(null)
  let error = $state('')
  let index = $state(0)
  let busy = $state(false)

  async function load() {
    try {
      systems = await bios.status()
      index = Math.min(index, Math.max(systems.length - 1, 0))
    } catch (e) {
      error = String(e)
    }
  }
  load()

  const ready = (s: BiosSystem) => s.files.filter((f) => f.state === 'present').length
  const missing = (s: BiosSystem) => s.files.filter((f) => f.state === 'missing').length
  const focused = $derived(systems?.[index])

  const LABEL: Record<BiosState, string> = {
    present: 'On the Deck',
    missing: 'Missing',
    different: 'Different file on the Deck (kept)',
    unsupported: 'Not placed by Romulus',
  }

  async function get() {
    const s = focused
    if (!s || busy || !s.supported || missing(s) === 0) return
    busy = true
    try {
      const r = await bios.download(s.platform_id)
      if (r.installed) toast(`${r.installed} BIOS file${r.installed === 1 ? '' : 's'} added for ${s.name}.`)
      for (const f of r.failed) toast(f, 'error', 7000)
      await load()
    } catch (e) {
      toast(String(e), 'error', 6000)
    } finally {
      busy = false
    }
  }

  const off = onAction(({ action }) => {
    const n = systems?.length ?? 0
    switch (action) {
      case 'up':
        index = Math.max(index - 1, 0)
        return true
      case 'down':
        index = Math.min(index + 1, Math.max(n - 1, 0))
        return true
      case 'a':
        get()
        return true
    }
    return false
  })
  onDestroy(off)

  const hints = $derived.by<[Action, string][]>(() => {
    const h: [Action, string][] = []
    if (focused?.supported && missing(focused) > 0) h.push(['a', busy ? 'Downloading…' : 'Get missing'])
    h.push(['b', 'Back'])
    return h
  })
</script>

<Header title="BIOS" subtitle="From RomM into RetroDECK's bios folder" />
<main>
  {#if error}
    <p class="empty">{error}</p>
  {:else if !systems}
    <p class="empty">Checking…</p>
  {:else if systems.length === 0}
    <p class="empty">RomM has no BIOS files. Add them in RomM under each platform's firmware.</p>
  {:else}
    <div class="list">
      {#each systems as s, i (s.platform_id)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="row" class:focused={i === index} role="button" tabindex="-1" onclick={() => (index === i ? get() : (index = i))}>
          <span class="name">{s.name}</span>
          <span class="count" class:ok={s.supported && missing(s) === 0}>
            {s.supported ? `${ready(s)} of ${s.files.length} ready` : 'not supported yet'}
          </span>
        </div>
      {/each}
    </div>
    {#if focused}
      <div class="files">
        {#each focused.files as f (f.id)}
          <div class="file">
            <span class="dot {f.state}"></span>
            <span class="fname">{f.name}</span>
            <span class="meta">{LABEL[f.state]} · {size(f.size_bytes)}{f.verified ? '' : ' · unverified dump'}</span>
          </div>
        {/each}
        <p class="note">Romulus only adds missing files. Files already in the bios folder are never replaced.</p>
      </div>
    {/if}
  {/if}
</main>
<Footer {hints} />

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
    color: var(--muted);
    font-size: 18px;
    margin: auto;
  }
  .list {
    width: 40%;
    max-width: 460px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow: hidden;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 16px;
    border-radius: 8px;
    font-size: 17px;
    color: var(--text-2);
  }
  .row.focused {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .count {
    font-size: 14px;
    color: var(--muted);
    white-space: nowrap;
  }
  .count.ok {
    color: var(--ok);
  }
  .row.focused .count {
    color: rgb(255 255 255 / 0.85);
  }
  .files {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow: hidden;
  }
  .file {
    display: grid;
    grid-template-columns: 12px 1fr;
    column-gap: 12px;
    align-items: center;
    padding: 8px 12px;
    border-radius: 8px;
    background: rgb(255 255 255 / 0.04);
  }
  .fname {
    font-size: 15px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    grid-column: 2;
    font-size: 13px;
    color: var(--muted);
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--muted);
  }
  .dot.present {
    background: var(--ok);
  }
  .dot.missing {
    background: var(--accent);
  }
  .dot.different {
    background: #c19a1c;
  }
  .note {
    color: var(--muted);
    font-size: 13px;
    margin: 6px 0 0;
  }
</style>
