<script lang="ts">
  // A RetroAchievements badge (served by RomM), loaded through the asset cache.
  import { assetUrl } from '../lib/api'

  let { path, size = 48, title = '' }: { path: string | null; size?: number; title?: string } = $props()
  let src = $state<string | null>(null)

  $effect(() => {
    const p = path
    let alive = true
    src = null
    if (p) assetUrl(p, () => alive).then((u) => alive && (src = u)).catch(() => {})
    return () => {
      alive = false
    }
  })
</script>

<div class="badge" style:width="{size}px" style:height="{size}px" {title}>
  {#if src}<img {src} alt="" decoding="async" />{/if}
</div>

<style>
  .badge {
    flex: none;
    border-radius: 8px;
    overflow: hidden;
    background: rgb(255 255 255 / 0.06);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
