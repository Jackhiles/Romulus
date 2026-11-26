<script lang="ts">
  // Cover art for a tile: loads lazily (only mounted tiles exist) and fades in. Shows the
  // title on a tinted card until the image arrives, or if there is none.
  import { invoke } from '@tauri-apps/api/core'
  import { assetUrl, assetUrlCached } from '../lib/api'

  let { path, title, focused = false }: { path: string | null; title: string; focused?: boolean } = $props()

  let src = $state<string | null>(null)
  let loaded = $state(false)

  $effect(() => {
    const p = path
    let alive = true
    src = assetUrlCached(p)
    loaded = false
    if (p && !src) {
      assetUrl(p, () => alive)
        .then((u) => {
          if (alive) src = u
        })
        .catch(() => {})
    }
    return () => {
      alive = false
    }
  })

  // A stable hue per title for the placeholder.
  const hue = $derived([...title].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7))
</script>

<div class="cover" class:focused style:--hue={hue}>
  {#if !loaded}
    <div class="placeholder"><span>{title}</span></div>
  {/if}
  {#if src}
    <img
      {src}
      alt=""
      class:loaded
      draggable="false"
      onload={() => (loaded = true)}
      onerror={() => invoke('log_ui', { level: 'warn', message: `cover failed to load: ${src}` }).catch(() => {})}
    />
  {/if}
</div>

<style>
  .cover {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--surface);
    transition: transform 120ms ease-out, box-shadow 120ms ease-out;
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.35);
  }
  .focused {
    transform: scale(1.07);
    box-shadow: 0 0 0 3px var(--accent), 0 10px 28px rgb(0 0 0 / 0.55);
    z-index: 1;
  }
  .placeholder {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: flex-end;
    padding: 10px;
    background: linear-gradient(160deg, hsl(var(--hue) 35% 26%), hsl(var(--hue) 30% 14%));
  }
  .placeholder span {
    font-size: 14px;
    font-weight: 600;
    line-height: 1.2;
    color: var(--text);
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
  }
  img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0;
    transition: opacity 160ms ease-out;
  }
  img.loaded {
    opacity: 1;
  }
</style>
