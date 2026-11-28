<script lang="ts">
  import { onDestroy } from 'svelte'
  import Backdrop from '../components/Backdrop.svelte'
  import Confirm from '../components/Confirm.svelte'
  import Badge from '../components/Badge.svelte'
  import SaveConflict from '../components/SaveConflict.svelte'
  import Cover from '../components/Cover.svelte'
  import Footer from '../components/Footer.svelte'
  import GameInfo from '../components/GameInfo.svelte'
  import Menu, { type MenuRow } from '../components/Menu.svelte'
  import { NO_USER, nextRating, nextStatus, readUser, stars, statusLabel, userTags, type RomUser } from '../lib/playStatus'
  import {
    achievements,
    api,
    collections,
    romUser,
    music,
    assetUrl,
    type Achievements,
    games,
    type InstallStatus,
    type Rom,
    type RomDetail,
    type SaveChoice,
    type SaveSyncStatus,
    type SaveConflict as Conflict,
  } from '../lib/api'
  import { device, progressText, removed, saveMessage, startDownload } from '../lib/play.svelte'
  import { screenshots } from '../lib/details.svelte'
  import { ago, fileTags, hueOf, size } from '../lib/format'
  import { onAction, type Action } from '../lib/input'
  import { push } from '../lib/nav.svelte'
  import { toast } from '../lib/toast.svelte'

  let { romId, siblings = [], surprise = false }: { romId: number; siblings?: number[]; surprise?: boolean } =
    $props()

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
  let versions = $state<Rom[]>([])
  let fav = $state(false)
  let favBusy = $state(false)
  let user = $state<RomUser>({ ...NO_USER })
  let menuOpen = $state(false)

  /** Change the play status on RomM: shown at once, undone if RomM refuses. */
  async function setUser(patch: Partial<RomUser>) {
    const want = id
    const before = user
    user = { ...user, ...patch }
    try {
      const now = readUser(await romUser.set(want, patch))
      if (want === id) user = now
    } catch (e) {
      if (want === id) user = before
      toast(String(e), 'error', 8000)
    }
  }

  const manualPath = $derived(
    rom?.data?.has_manual && typeof rom.data.path_manual === 'string' && rom.data.path_manual
      ? `/assets/romm/resources/${rom.data.path_manual.replace(/^\/+/, '')}`
      : null,
  )

  const menuRows = $derived.by<MenuRow[]>(() => {
    const rows: MenuRow[] = [
      { label: 'Favourite', value: fav ? 'Yes' : 'No', run: toggleFavourite, stay: true },
      { label: 'Now playing', value: user.now_playing ? 'Yes' : 'No', run: () => setUser({ now_playing: !user.now_playing }), stay: true },
      { label: 'Backlog', value: user.backlogged ? 'Yes' : 'No', run: () => setUser({ backlogged: !user.backlogged }), stay: true },
      { label: 'Status', value: statusLabel(user.status), cycle: (d) => setUser({ status: nextStatus(user.status, d) }) },
      { label: 'Your rating', value: stars(user.rating), cycle: (d) => setUser({ rating: nextRating(user.rating, d) }) },
    ]
    if (manualPath && rom) {
      const title = rom.title
      const path = manualPath
      rows.push({ label: 'Manual', value: 'Read', run: () => push({ name: 'manual', title, path }) })
    }
    return rows
  })

  $effect(() => {
    const want = id
    collections
      .isFavourite(want)
      .then((f) => {
        if (want === id) fav = f
      })
      .catch(() => {})
  })

  async function toggleFavourite() {
    if (favBusy) return
    favBusy = true
    const want = id
    try {
      const now = await collections.setFavourite(want, !fav)
      if (want === id) fav = now
      toast(now ? 'Added to Favourites' : 'Removed from Favourites')
    } catch (e) {
      toast(String(e), 'error', 8000)
    } finally {
      favBusy = false
    }
  }

  $effect(() => {
    const want = id
    api
      .versions(want)
      .then((v) => {
        // Keep the list while switching between its versions.
        if (want === id && !versions.some((x) => x.id === want)) versions = v
      })
      .catch(() => {})
  })
  const vIndex = $derived(versions.findIndex((v) => v.id === id))

  function switchVersion(delta: number) {
    if (versions.length < 2) return
    const i = vIndex < 0 ? 0 : vIndex
    id = versions[(i + delta + versions.length) % versions.length].id
  }

  // The game's music (kept if it was already playing from the list).
  $effect(() => {
    const want = id
    const t = setTimeout(() => music.preview(want).catch(() => {}), 500)
    return () => clearTimeout(t)
  })

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
      user = readUser(r?.data?.rom_user)
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
        if (r.states) toast(`Restored ${r.states === 1 ? 'a save state' : `${r.states} save states`} from RomM.`, 'info', 5000)
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
      case 'lt':
      case 'rt':
        switchVersion(action === 'rt' ? 1 : -1)
        return true
      case 'start':
        if (rom) menuOpen = true
        return true
      case 'select':
        // Opened by "Surprise me": another random game.
        if (surprise) api.randomRom().then((r) => r && (id = r.id))
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
    if (versions.length > 1) h.push(['rt', 'Version'])
    if (surprise) h.push(['select', 'Another'])
    if (rom) h.push(['start', 'Menu'])
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
        {#if versions.length > 1}
          <div class="version">
            Version {vIndex + 1} of {versions.length}{fileTags(rom.fs_name) ? ` · ${fileTags(rom.fs_name)}` : ''} · LT/RT to switch
          </div>
        {/if}
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
            {#if fav}<span class="fav">★ Favourite</span>{/if}
            {#each userTags(user) as t (t)}<span class="user-tag">{t}</span>{/each}
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
        {#if sync?.states}<div class="sync-line">{sync.states}</div>{/if}
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
{#if menuOpen && rom}
  <Menu title={rom.title} rows={menuRows} onclose={() => (menuOpen = false)} />
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
  .version {
    color: var(--accent);
    font-size: 14px;
    font-weight: 600;
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
  .fav {
    color: #f5c451;
    font-size: 14px;
  }
  .user-tag {
    font-size: 13px;
    padding: 2px 10px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.1);
    color: var(--text-2);
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
