<script lang="ts">
  import { onDestroy } from 'svelte'
  import Cover from '../components/Cover.svelte'
  import Footer from '../components/Footer.svelte'
  import { api, assetUrl, type RomDetail } from '../lib/api'
  import { size } from '../lib/format'
  import { onAction, type Action } from '../lib/input'
  import { toast } from '../lib/toast.svelte'

  let { romId, siblings = [] }: { romId: number; siblings?: number[] } = $props()

  let id = $state(0)
  let rom = $state<RomDetail | null>(null)
  let shots = $state<string[]>([])
  let shot = $state(0)

  $effect(() => {
    id = romId
  })

  $effect(() => {
    const want = id
    if (!want) return
    api.rom(want).then((r) => {
      if (want !== id) return
      rom = r
      shot = 0
      shots = []
      const paths: string[] = (r?.data?.merged_screenshots ?? []).filter((p: unknown) => typeof p === 'string' && p.startsWith('/'))
      Promise.all(paths.slice(0, 8).map((p) => assetUrl(p, () => want === id).catch(() => null))).then((urls) => {
        if (want === id) shots = urls.filter((u): u is string => !!u)
      })
    })
  })

  const meta = $derived(rom?.data?.metadatum ?? {})
  const year = $derived.by(() => {
    const t = meta.first_release_date
    if (!t) return null
    return new Date(t > 1e11 ? t : t * 1000).getUTCFullYear()
  })
  const facts = $derived(
    [
      year && ['Released', String(year)],
      meta.genres?.length && ['Genre', meta.genres.slice(0, 3).join(', ')],
      (meta.developers?.length || meta.companies?.length) && ['Developer', (meta.developers?.length ? meta.developers : meta.companies).slice(0, 2).join(', ')],
      meta.player_count && ['Players', meta.player_count],
      rom?.data?.regions?.length && ['Region', rom.data.regions.join(', ')],
      rom && ['File', `${rom.fs_name} · ${size(rom.size_bytes)}`],
    ].filter(Boolean) as [string, string][],
  )

  function sibling(delta: number) {
    const i = siblings.indexOf(id)
    if (i < 0) return
    const next = siblings[i + delta]
    if (next) id = next
  }

  const off = onAction(({ action }) => {
    switch (action) {
      case 'a':
        toast('Downloading and playing arrive in Phase 2.')
        return true
      case 'lb':
        sibling(-1)
        return true
      case 'rb':
        sibling(1)
        return true
      case 'left':
        if (shots.length) shot = (shot - 1 + shots.length) % shots.length
        return true
      case 'right':
        if (shots.length) shot = (shot + 1) % shots.length
        return true
    }
    return false
  })
  onDestroy(off)

  const hints = $derived.by(() => {
    const h: [Action, string][] = [['a', 'Play']]
    if (shots.length > 1) h.push(['right', 'Screenshots'])
    if (siblings.length > 1) h.push(['rb', 'Next game'])
    h.push(['b', 'Back'])
    return h
  })
</script>

<div class="detail">
  {#if shots[shot]}
    <div class="backdrop" style:background-image="url({shots[shot]})"></div>
  {/if}
  {#if rom}
    <div class="cover"><Cover path={rom.cover_large ?? rom.cover_small} title={rom.title} /></div>
    <div class="info">
      <h1>{rom.title}</h1>
      <dl>
        {#each facts as [k, v] (k)}
          <dt>{k}</dt>
          <dd>{v}</dd>
        {/each}
      </dl>
      {#if rom.data?.summary}<p class="summary">{rom.data.summary}</p>{/if}
      <div class="actions">
        <button class="primary focused" tabindex="-1" onclick={() => toast('Downloading and playing arrive in Phase 2.')}>▶ Play</button>
      </div>
      {#if shots.length > 1}
        <div class="dots">
          {#each shots as _, i (i)}<span class:on={i === shot}></span>{/each}
        </div>
      {/if}
    </div>
  {/if}
</div>
<Footer {hints} />

<style>
  .detail {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 40px;
    padding: 40px 48px 10px;
    position: relative;
    overflow: hidden;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    background-size: cover;
    background-position: center;
    opacity: 0.16;
    transition: background-image 200ms;
  }
  .cover {
    position: relative;
    flex: none;
    width: 300px;
    height: 400px;
  }
  .info {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h1 {
    margin: 0;
    font-size: 34px;
    line-height: 1.15;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 18px;
    margin: 0;
    font-size: 15px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .summary {
    margin: 0;
    font-size: 15px;
    line-height: 1.55;
    color: #c9ccd4;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 6;
    line-clamp: 6;
    -webkit-box-orient: vertical;
  }
  .actions {
    margin-top: auto;
  }
  .dots {
    display: flex;
    gap: 6px;
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
