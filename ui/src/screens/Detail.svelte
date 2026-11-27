<script lang="ts">
  import { onDestroy } from 'svelte'
  import Backdrop from '../components/Backdrop.svelte'
  import Cover from '../components/Cover.svelte'
  import Footer from '../components/Footer.svelte'
  import GameInfo from '../components/GameInfo.svelte'
  import { api, assetUrl, type RomDetail } from '../lib/api'
  import { screenshots } from '../lib/details.svelte'
  import { hueOf, size } from '../lib/format'
  import { onAction, type Action } from '../lib/input'
  import { toast } from '../lib/toast.svelte'

  let { romId, siblings = [] }: { romId: number; siblings?: number[] } = $props()

  // svelte-ignore state_referenced_locally
  let id = $state(romId)
  let rom = $state<RomDetail | null>(null)
  let shots = $state<string[]>([])
  let shot = $state(0)

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

  function sibling(delta: number) {
    const i = siblings.indexOf(id)
    const next = i >= 0 ? siblings[i + delta] : undefined
    if (next) id = next
  }

  function play() {
    toast('Downloading and playing arrive in Phase 2.')
  }

  const off = onAction(({ action }) => {
    switch (action) {
      case 'a':
        play()
        return true
      case 'lb':
      case 'rb':
        sibling(action === 'rb' ? 1 : -1)
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
  const hints = $derived.by(() => {
    const h: [Action, string][] = [['a', 'Play']]
    if (shots.length > 1) h.push(['right', 'Screenshots'])
    if (siblings.length > 1) h.push(['rb', 'Next game'])
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
          <button class="play" tabindex="-1" onclick={play}>▶ Play</button>
          {#if shots.length > 1}
            <div class="dots">
              {#each shots as _, i (i)}<span class:on={i === shot}></span>{/each}
            </div>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>
<Footer {hints} />

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
