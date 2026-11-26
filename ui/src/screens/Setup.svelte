<script lang="ts">
  // First run and sign-in: server address, then device pairing (approve on a phone), or
  // username and password as a fallback.
  import qrcode from 'qrcode-generator'
  import { onDestroy } from 'svelte'
  import Footer from '../components/Footer.svelte'
  import { api, type PairInfo } from '../lib/api'
  import { APP_NAME } from '../lib/app'
  import { onAction, type Action } from '../lib/input'
  import { toast } from '../lib/toast.svelte'

  let { initialServer = '', ondone }: { initialServer?: string; ondone: () => void } = $props()

  type Step = 'server' | 'pair' | 'password'
  // svelte-ignore state_referenced_locally
  let step = $state<Step>(initialServer ? 'pair' : 'server')
  // svelte-ignore state_referenced_locally
  let server = $state(initialServer)
  let serverVersion = $state('')
  let busy = $state(false)
  let error = $state('')
  let pair = $state<PairInfo | null>(null)
  let pairState = $state('')
  let username = $state('')
  let password = $state('')
  let focus = $state(0)
  let fields: (HTMLElement | undefined)[] = $state([])
  let pollTimer: ReturnType<typeof setTimeout> | undefined

  const qrSvg = $derived.by(() => {
    if (!pair) return ''
    const q = qrcode(0, 'M')
    q.addData(pair.url)
    q.make()
    return q.createSvgTag({ cellSize: 5, margin: 2, scalable: true })
  })

  function focusField(i: number) {
    const els = fields.filter(Boolean) as HTMLElement[]
    if (els.length === 0) return
    focus = Math.min(Math.max(i, 0), els.length - 1)
    els[focus].focus()
  }

  $effect(() => {
    void step
    queueMicrotask(() => focusField(0))
  })

  async function submitServer() {
    if (!server.trim() || busy) return
    busy = true
    error = ''
    try {
      const info = await api.setServer(server)
      server = info.url
      serverVersion = info.version
      step = 'pair'
      startPairing()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  async function startPairing() {
    clearTimeout(pollTimer)
    error = ''
    pairState = ''
    pair = null
    try {
      pair = await api.pairStart()
      schedulePoll(pair.interval)
    } catch (e) {
      error = `Pairing is not available: ${e}. Sign in with your username and password instead.`
    }
  }

  function schedulePoll(seconds: number) {
    clearTimeout(pollTimer)
    pollTimer = setTimeout(poll, Math.max(seconds, 2) * 1000)
  }

  async function poll() {
    if (!pair || step !== 'pair') return
    try {
      const r = await api.pairPoll(pair.device_code)
      if (r === 'approved') {
        toast('Signed in to RomM')
        ondone()
      } else if (r === 'pending') schedulePoll(pair.interval)
      else if (r === 'slow_down') schedulePoll(pair.interval + 5)
      else if (r === 'denied') pairState = 'The request was denied in RomM.'
      else pairState = 'The code expired.'
    } catch (e) {
      pairState = `Could not check: ${e}`
      schedulePoll(pair.interval * 2)
    }
  }

  async function submitPassword() {
    if (busy) return
    busy = true
    error = ''
    try {
      await api.passwordLogin(username, password)
      password = ''
      toast('Signed in to RomM')
      ondone()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  $effect(() => {
    if (step === 'pair' && initialServer && !pair) startPairing()
  })

  onDestroy(() => clearTimeout(pollTimer))

  function activate() {
    const el = fields.filter(Boolean)[focus] as HTMLElement | undefined
    if (!el) return
    if (el instanceof HTMLInputElement) {
      el.focus()
      api.openKeyboard()
      // Enter in a field moves on.
      return
    }
    el.click()
  }

  const off = onAction(({ action }: { action: Action }) => {
    switch (action) {
      case 'up':
        focusField(focus - 1)
        return true
      case 'down':
        focusField(focus + 1)
        return true
      case 'a': {
        const el = fields.filter(Boolean)[focus]
        // Enter on a text field submits the step.
        if (el instanceof HTMLInputElement && document.activeElement === el && el.value) {
          if (step === 'server') submitServer()
          else if (step === 'password') {
            if (el.type === 'password') submitPassword()
            else focusField(focus + 1)
          }
          return true
        }
        activate()
        return true
      }
      case 'x':
        if (step === 'pair') step = 'password'
        else if (step === 'password') {
          step = 'pair'
          startPairing()
        }
        return true
      case 'b':
        if (step !== 'server') {
          clearTimeout(pollTimer)
          step = 'server'
        }
        return true
    }
    return false
  })
  onDestroy(off)

  const hints = $derived<[Action, string][]>(
    step === 'server'
      ? [['a', 'Select']]
      : step === 'pair'
        ? [['x', 'Use password'], ['b', 'Change server']]
        : [['a', 'Select'], ['x', 'Pair instead'], ['b', 'Change server']],
  )
</script>

<div class="setup">
  <div class="panel">
    <h1>{APP_NAME}</h1>

    {#if step === 'server'}
      <p class="lead">Enter the address of your RomM server.</p>
      <label>
        <span>Server address</span>
        <input
          bind:this={fields[0]}
          bind:value={server}
          type="url"
          placeholder="romm.example.com or 192.168.1.10:8080"
          autocapitalize="off"
          spellcheck="false"
          onfocus={() => (focus = 0)}
        />
      </label>
      <button bind:this={fields[1]} class="primary" disabled={busy} onclick={submitServer} onfocus={() => (focus = 1)}>
        {busy ? 'Connecting…' : 'Continue'}
      </button>
    {:else if step === 'pair'}
      <p class="lead">
        Scan the code with your phone, or open <strong>{pair?.url ?? '…'}</strong>, sign in to RomM and approve this
        device.
      </p>
      <div class="pair">
        <div class="qr">{@html qrSvg}</div>
        <div>
          <div class="code-label">Pairing code</div>
          <div class="code">{pair?.user_code ?? '…'}</div>
          <div class="muted">{pairState || 'Waiting for approval…'}</div>
          {#if pairState}
            <button bind:this={fields[0]} class="primary" onclick={startPairing} onfocus={() => (focus = 0)}>
              Get a new code
            </button>
          {/if}
        </div>
      </div>
      <p class="muted small">{server}{serverVersion ? ` · RomM ${serverVersion}` : ''}</p>
    {:else}
      <p class="lead">Sign in with your RomM account.</p>
      <label>
        <span>Username</span>
        <input
          bind:this={fields[0]}
          bind:value={username}
          autocapitalize="off"
          spellcheck="false"
          onfocus={() => (focus = 0)}
        />
      </label>
      <label>
        <span>Password</span>
        <input bind:this={fields[1]} bind:value={password} type="password" onfocus={() => (focus = 1)} />
      </label>
      <button bind:this={fields[2]} class="primary" disabled={busy} onclick={submitPassword} onfocus={() => (focus = 2)}>
        {busy ? 'Signing in…' : 'Sign in'}
      </button>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}
  </div>
</div>
<Footer {hints} />

<style>
  .setup {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .panel {
    width: min(640px, 90vw);
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  h1 {
    margin: 0;
    font-size: 40px;
    background: linear-gradient(90deg, #ff7a45, #e0457b);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .lead {
    margin: 0;
    font-size: 18px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 14px;
    color: var(--muted);
  }
  .pair {
    display: flex;
    gap: 28px;
    align-items: center;
  }
  .qr {
    width: 200px;
    height: 200px;
    background: #fff;
    border-radius: 12px;
    padding: 6px;
    flex: none;
  }
  .qr :global(svg) {
    width: 100%;
    height: 100%;
  }
  .code-label {
    color: var(--muted);
    font-size: 14px;
  }
  .code {
    font-size: 40px;
    font-weight: 800;
    letter-spacing: 3px;
    margin: 4px 0 10px;
    font-variant-numeric: tabular-nums;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 13px;
    margin: 0;
  }
  .error {
    color: var(--danger);
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
