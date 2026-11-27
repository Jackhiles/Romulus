<script lang="ts">
  import { onDestroy } from 'svelte'
  import Backdrop from '../components/Backdrop.svelte'
  import Confirm from '../components/Confirm.svelte'
  import Badge from '../components/Badge.svelte'
  import SaveConflict from '../components/SaveConflict.svelte'
  import Cover from '../components/Cover.svelte'
  import Footer from '../components/Footer.svelte'
  import GameInfo from '../components/GameInfo.svelte'
  import {
    achievements,
    api,
    assetUrl,
    type Achievements,
    games,
    type InstallStatus,
    type RomDetail,
    type SaveChoice,
    type SaveSyncStatus,
    type SaveConflict as Conflict,
  } from '../lib/api'
  import { device, progressText, removed, saveMessage, startDownload } from '../lib/play.svelte'
  import { screenshots } from '../lib/details.svelte'
  import { ago, hueOf, size } from '../lib/format'
  import { onAction, type Action } from '../lib/input'
  import { push } from '../lib/nav.svelte'
  import { toast } from '../lib/toast.svelte'

  let { romId, siblings = [] }: { romId: number; siblings?: number[] } = $props()

  // svelte-ignore state_referenced_locally
  let id = $state(romId)
  let rom = $state<RomDetail | null>(null)
  let shots = $state<string[]>([])
  let shot = $state(0)
  let status = $state<InstallStatus | null>(null)
  let confirmRemove = $state(false)
  let conflict = $state<Conflict | null>(null)
  let starting = $state(false)
  let sync = $state<SaveSyncStatus | null>(null)
  let ra = $state<Achievements | null>(null)

  $effect(() => {
    const want = id
    ra = null
    achievements(want)
      .then((a) => {
        if (want === id) ra = a
      })
      .catch(() => {})
  })

  $effect(() => {
    const want = id
    void device.revision
    sync = null
    if (status?.state !== 'installed') return
    games
      .saveSyncStatus(want)
      .then((n) => {
        if (want === id) sync = n
      })
      .catch(() => {})
  })

  // Re-check whenever an install changes (download finished, removed, ...).
  $effect(() => {
    void device.revision
    const want = id
    games
      .status(want)
      .then((s) => {
        if (want === id) status = s
      })
      .catch((e) => toast(String(e), 'error'))
  })
  const progress = $derived(device.downloads[id])
  const busy = $derived(!!progress || status?.state === 'downloading')

  $effect(() => {
    const want = id
    api.rom(want).then((r) => {
      if (want !== id) return
      rom = r
      shot = 0
      shots = []
      const paths = screenshots(r).slice(0, 8)
      Promise.all(paths.map((p) => assetUrl(p, () => want === id).catch(() => null))).then((urls) => {
        if (want === id) shots = urls.filter((u): u is string => !!u)
      })
    })
  })

  function syncText(s: SaveSyncStatus): string {
    if (!s.syncs) return s.note ?? ''
    if (s.note) return s.note
    if (s.pending) return 'Saves sync with RomM · newer save on this Deck, uploading soon'
    return s.synced_at ? `Saves sync with RomM · synced ${ago(Date.parse(s.synced_at) / 1000)}` : 'Saves sync with RomM'
  }

  function sibling(delta: number) {
    const i = siblings.indexOf(id)
    const next = i >= 0 ? siblings[i + delta] : undefined
    if (next) id = next
  }

  function primary() {
    if (!rom || busy || !status) return
    if (status.state === 'installed') {
      play()
    } else if (status.state === 'not_installed') {
      startDownload(id)
    } else if (status.state === 'unavailable') {
      toast(status.reason, 'error', 5000)
    }
  }

  /** Start the game. Saves sync first; a conflict asks before anything is launched. */
  async function play(choice?: SaveChoice) {
    if (!rom || starting) return
    conflict = null
    starting = true
    device.playingTitle = rom.title
    try {
      const r = await games.play(id, choice)
      if (r.state === 'conflict') conflict = r.info
      else {
        const m = saveMessage(r.save, 'before')
        if (m) toast(m, r.save?.startsWith('failed') ? 'error' : 'info', 5000)
        if (r.warning) toast(r.warning, 'error', 15000)
      }
    } catch (e) {
      toast(String(e), 'error', 6000)
    } finally {
      starting = false
    }
  }

  async function remove() {
    confirmRemove = false
    try {
      await games.remove(id)
      removed(id)
      toast('Removed from this Deck. Your saves were not touched.')
    } catch (e) {
      toast(String(e), 'error', 6000)
    }
  }

  const off = onAction(({ action }) => {
    switch (action) {
      case 'a':
        primary()
        return true
      case 'x':
        if (busy) games.cancel(id)
        else if (status?.state === 'installed') confirmRemove = true
        return true
      case 'lb':
      case 'rb':
        sibling(action === 'rb' ? 1 : -1)
        return true
      case 'y':
        if (ra && rom) push({ name: 'achievements', romId: id, title: rom.title })
        return true
      case 'left':
      case 'right':
        if (shots.length > 1) shot = (shot + (action === 'right' ? 1 : -1) + shots.length) % shots.length
        return true
    }
    return false
  })
  onDestroy(off)

  const platform = $derived(typeof rom?.data?.platform_display_name === 'string' ? rom.data.platform_display_name : '')
  const primaryLabel = $derived.by(() => {
    if (busy) return 'Downloading…'
    if (starting) return 'Syncing save…'
    switch (status?.state) {
      case 'installed':
        return '▶ Play'
      case 'not_installed':
        return `⬇ Download${rom && rom.size_bytes >= 1024 ? ` · ${size(rom.size_bytes)}` : ''}`
      case 'unavailable':
        return 'Not available'
      default:
        return '…'
    }
  })

  const hints = $derived.by(() => {
    const h: [Action, string][] = []
    if (status?.state === 'installed') h.push(['a', 'Play'], ['x', 'Remove'])
    else if (busy) h.push(['x', progress?.queued ? 'Remove from queue' : 'Stop download'])
    else if (status?.state === 'not_installed') h.push(['a', 'Download'])
    if (shots.length > 1) h.push(['right', 'Screenshots'])
    if (siblings.length > 1) h.push(['rb', 'Next game'])
    if (ra) h.push(['y', 'Achievements'])
    h.push(['b', 'Back'])
    return h
  })
</script>

<div class="detail">
  <Backdrop src={shots[shot] ?? null} hue={hueOf(String(rom?.data?.platform_slug ?? ''))} strength={0.3} />
  {#if rom}
    <div class="body">
      <div class="cover"><Cover path={rom.cover_large ?? rom.cover_small} title={rom.title} /></div>
      <div class="info">
        {#if platform}<div class="platform">{platform}</div>{/if}
        <GameInfo title={rom.title} detail={rom} lines={7} size="lg" />
        <!-- Multi-disc games report the size of their small playlist file only. -->
        <div class="file">{rom.fs_name}{rom.size_bytes >= 1024 ? ` · ${size(rom.size_bytes)}` : ''}</div>
        <div class="actions">
          {#if busy}
            <div class="progress">
              <div class="bar"><div class="fill" style:transform="scaleX({progress?.total ? progress.done / progress.total : 0})"></div></div>
              <span>{progressText(progress ?? { done: 0, total: 0 })}</span>
            </div>
          {:else}
            <button
              class="play"
              class:disabled={status?.state === 'unavailable'}
              tabindex="-1"
              onclick={primary}>{primaryLabel}</button
            >
            {#if status?.state === 'installed'}
              <span class="on-device">On this Deck</span>
            {/if}
            {#if status?.state === 'unavailable'}<span class="reason">{status.reason}</span>{/if}
          {/if}
          {#if shots.length > 1}
            <div class="dots">
              {#each shots as _, i (i)}<span class:on={i === shot}></span>{/each}
            </div>
          {/if}
        </div>
        <div class="sync-line" class:warn={!!sync?.note && sync.syncs}>
          {#if sync}{syncText(sync)}{/if}
        </div>
        {#if ra}
          <div class="ra">
            <div class="ra-sum">
              <span class="trophy">🏆</span>
              {ra.earned} of {ra.total} achievements · {ra.points_earned} / {ra.points_total} points
            </div>
            <div class="badges">
              {#each ra.list.slice(0, 12) as a, i (i)}<Badge path={a.badge} size={40} title={a.title} />{/each}
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
<Footer {hints} />
{#if conflict && rom}
  <SaveConflict title={rom.title} info={conflict} onchoose={(c) => play(c)} oncancel={() => (conflict = null)} />
{/if}
{#if confirmRemove && rom}
  <Confirm
    title="Remove from this Deck?"
    message={`${rom.title} will be deleted from the SD card. Your saves and save states stay, and you can download it again any time.`}
    confirm="Remove"
    onconfirm={remove}
    oncancel={() => (confirmRemove = false)}
  />
{/if}

<style>
  .detail {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .body {
    position: relative;
    z-index: 1;
    height: 100%;
    display: flex;
    gap: 44px;
    padding: 56px 56px 16px;
  }
  .cover {
    flex: none;
    width: 300px;
    height: 400px;
  }
  .info {
    flex: 1;
    min-width: 0;
    max-width: 720px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .platform {
    font-size: 13px;
    font-weight: 800;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--accent);
  }
  .file {
    color: var(--muted);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .actions {
    margin-top: 12px;
    display: flex;
    align-items: center;
    gap: 24px;
  }
  .play {
    padding: 14px 40px;
    font-size: 19px;
    font-weight: 700;
    color: #fff;
    background: var(--accent);
    border: 0;
    border-radius: 12px;
    box-shadow:
      0 0 0 3px var(--bg),
      0 0 0 6px var(--accent);
    cursor: pointer;
  }
  .play.disabled {
    background: rgb(255 255 255 / 0.12);
    box-shadow: none;
    color: var(--muted);
  }
  .ra {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ra-sum {
    color: var(--text-2);
    font-size: 14px;
  }
  .trophy {
    margin-right: 4px;
  }
  .badges {
    display: flex;
    gap: 8px;
    overflow: hidden;
  }
  .sync-line {
    min-height: 20px;
    color: var(--muted);
    font-size: 14px;
  }
  .sync-line.warn {
    color: var(--danger);
  }
  .on-device,
  .reason {
    color: var(--muted);
    font-size: 14px;
  }
  .progress {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 360px;
    color: var(--text-2);
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    height: 10px;
    border-radius: 5px;
    background: rgb(255 255 255 / 0.12);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transform-origin: left;
    transition: transform 240ms linear;
  }
  .dots {
    display: flex;
    gap: 7px;
  }
  .dots span {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: rgb(255 255 255 / 0.25);
  }
  .dots .on {
    background: var(--accent);
  }
</style>
