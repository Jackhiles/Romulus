<script lang="ts">
  import { writeText } from '@tauri-apps/plugin-clipboard-manager'
  import { onDestroy } from 'svelte'
  import Footer from '../components/Footer.svelte'
  import { api, onUpdateProgress, type Release, type Settings, type Status } from '../lib/api'
  import { ago, size } from '../lib/format'
  import { onAction, setWebGamepad, type Action } from '../lib/input'
  import { library, refresh } from '../lib/library.svelte'
  import { toast } from '../lib/toast.svelte'

  let { onsignout }: { onsignout: () => void } = $props()

  let status = $state<Status | null>(null)
  let settings = $state<Settings | null>(null)
  let release = $state<Release | null>(null)
  let checking = $state(false)
  let installing = $state<string | null>(null)
  let installed = $state(false)
  // Start on "Library", the first row that does something.
  let index = $state(2)
  let rowsEl: HTMLElement | undefined = $state()

  const LEVELS = ['error', 'warn', 'info', 'debug', 'trace']

  async function load() {
    ;[status, settings] = await Promise.all([api.status(), api.settings()])
  }
  load()

  async function patch(p: Partial<Settings>) {
    await api.updateSettings(p)
    if (p.web_gamepad !== undefined) setWebGamepad(p.web_gamepad)
    await load()
  }

  async function checkUpdate() {
    checking = true
    try {
      release = await api.checkUpdate()
      if (!release) toast(`You have the latest version (${status?.version}).`)
    } catch (e) {
      toast(`Could not check for updates: ${e}`, 'error')
    } finally {
      checking = false
    }
  }

  async function installUpdate() {
    if (!release || installing) return
    installing = 'Starting…'
    const off = await onUpdateProgress((done, total) => {
      installing = total ? `${Math.floor((done / total) * 100)}%` : size(done)
    })
    try {
      await api.installUpdate()
      installed = true
      toast(`Version ${release.version} installed. Press A to restart.`)
    } catch (e) {
      toast(`Update failed: ${e}`, 'error', 6000)
    } finally {
      off()
      installing = null
    }
  }

  let addingToSteam = false
  async function addToSteam() {
    if (addingToSteam || status?.in_steam) return
    addingToSteam = true
    toast('Adding to Steam…')
    try {
      toast(await api.addToSteam(), 'info', 6000)
      await load()
    } catch (e) {
      toast(String(e), 'error', 7000)
    } finally {
      addingToSteam = false
    }
  }

  async function copyDebug() {
    try {
      await writeText(await api.debugInfo())
      toast('Debug info copied to the clipboard.')
    } catch (e) {
      toast(`Could not copy: ${e}`, 'error')
    }
  }

  interface Row {
    label: string
    value: string
    run?: () => void
    cycle?: (dir: 1 | -1) => void
  }

  const rows = $derived.by<Row[]>(() => {
    if (!status || !settings) return []
    const s = settings
    const updateRow: Row = installed
      ? { label: 'Update', value: 'Restart to finish', run: () => api.restartApp() }
      : release
        ? { label: 'Update', value: installing ? `Installing ${installing}` : `Install version ${release.version}`, run: installUpdate }
        : { label: 'Updates', value: checking ? 'Checking…' : `Version ${status.version} · check now`, run: checkUpdate }
    if (!status.updatable && !installed && release) updateRow.value = `Version ${release.version} available (updates only work in the AppImage)`
    return [
      { label: 'Server', value: status.server_url ?? 'not set' },
      {
        label: 'Account',
        value: status.signed_in
          ? `Signed in${s.username ? ` as ${s.username}` : ' (paired device)'}`
          : 'Not signed in',
      },
      {
        label: 'Library',
        value: library.refreshing ? library.progress : `${status.rom_count} games · synced ${ago(library.lastSync ?? status.last_sync_at)} · sync now`,
        run: () => refresh(),
      },
      updateRow,
      { label: 'Check for updates on start', value: s.check_updates ? 'On' : 'Off', cycle: () => patch({ check_updates: !s.check_updates }) },
      { label: 'Fullscreen', value: s.fullscreen ? 'On' : 'Off', cycle: () => patch({ fullscreen: !s.fullscreen }) },
      {
        label: 'Controller input',
        value: s.web_gamepad ? 'Browser Gamepad API (fallback)' : 'Native (recommended)',
        cycle: () => patch({ web_gamepad: !s.web_gamepad }),
      },
      {
        label: 'Log level',
        value: s.log_level,
        cycle: (d) => patch({ log_level: LEVELS[(LEVELS.indexOf(s.log_level) + d + LEVELS.length) % LEVELS.length] }),
      },
      {
        label: 'Add to Steam',
        value: status.in_steam
          ? 'Already in your Steam library'
          : status.gaming_mode
            ? 'Do this once from Desktop Mode'
            : 'Add one shortcut for Romulus (restarts Steam)',
        run: addToSteam,
      },
      { label: 'Copy debug info', value: 'Versions, paths and recent log lines', run: copyDebug },
      { label: 'Sign out', value: 'Forget this device’s RomM credentials', run: async () => { await api.signOut(); onsignout() } },
      { label: `Quit ${status.app_name}`, value: '', run: () => api.quit() },
    ]
  })

  $effect(() => {
    const el = rowsEl?.children[index] as HTMLElement | undefined
    el?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  })

  const off = onAction(({ action }) => {
    const r = rows[index]
    switch (action) {
      case 'up':
        index = Math.max(index - 1, 0)
        return true
      case 'down':
        index = Math.min(index + 1, rows.length - 1)
        return true
      case 'left':
      case 'right':
        r?.cycle?.(action === 'right' ? 1 : -1)
        return true
      case 'a':
        if (r?.run) r.run()
        else r?.cycle?.(1)
        return true
      case 'start':
        return true
    }
    return false
  })
  onDestroy(off)

  const hints = $derived.by<[Action, string][]>(() => {
    const r = rows[index]
    const h: [Action, string][] = []
    if (r?.run) h.push(['a', 'Select'])
    if (r?.cycle) h.push(['a', 'Change'])
    h.push(['b', 'Back'])
    return h
  })
</script>

<header><h1>Settings</h1></header>
<main bind:this={rowsEl}>
  {#each rows as row, i (row.label)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="row"
      class:focused={i === index}
      class:action={!!row.run || !!row.cycle}
      role="button"
      tabindex="-1"
      onclick={() => {
        index = i
        if (row.run) row.run()
        else row.cycle?.(1)
      }}
    >
      <span class="label">{row.label}</span>
      <span class="value">{#if row.cycle}‹ {row.value} ›{:else}{row.value}{/if}</span>
    </div>
  {/each}
</main>
<Footer {hints} />

<style>
  header {
    padding: 22px 32px 8px;
  }
  h1 {
    margin: 0;
    font-size: 28px;
  }
  main {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 8px 32px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 14px 18px;
    border-radius: 10px;
    background: var(--surface);
    flex: none;
  }
  .row.focused {
    background: var(--surface-2);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .label {
    font-size: 17px;
    font-weight: 600;
    white-space: nowrap;
  }
  .value {
    color: var(--muted);
    font-size: 15px;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row:not(.action) .label {
    color: var(--muted);
  }
</style>
