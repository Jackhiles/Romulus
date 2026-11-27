<script lang="ts">
  // Title, facts, rating and description for one game. Used by the list panel, the grid
  // header and the detail page.
  import type { RomDetail } from '../lib/api'
  import { gameFacts } from '../lib/format'
  import Rating from './Rating.svelte'

  let {
    title,
    detail,
    lines = 5,
    size = 'md',
  }: { title: string; detail: RomDetail | null; lines?: number; size?: 'md' | 'lg' } = $props()

  const facts = $derived(gameFacts(detail?.data))
  const line = $derived([facts.year, facts.developer, facts.genre].filter(Boolean).join('  ·  '))
  const summary = $derived(typeof detail?.data?.summary === 'string' ? detail.data.summary : '')
</script>

<div class="info {size}">
  <h2>{title}</h2>
  {#if line || facts.players || facts.rating}
    <div class="facts">
      {#if line}<span>{line}</span>{/if}
      {#if facts.players}<span class="pill">{facts.players} {facts.players === '1' ? 'player' : 'players'}</span>{/if}
      {#if facts.rating}<Rating value={facts.rating} />{/if}
    </div>
  {/if}
  {#if summary && lines > 0}
    <p style:-webkit-line-clamp={lines} style:line-clamp={lines}>{summary}</p>
  {/if}
</div>

<style>
  .info {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 26px;
    font-weight: 700;
    line-height: 1.15;
    letter-spacing: -0.01em;
  }
  .lg h2 {
    font-size: 36px;
  }
  .facts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
    color: var(--muted);
    font-size: 15px;
  }
  .pill {
    padding: 2px 9px;
    border-radius: 999px;
    border: 1px solid rgb(255 255 255 / 0.16);
    font-size: 13px;
  }
  p {
    margin: 0;
    font-size: 15px;
    line-height: 1.55;
    color: var(--text-2);
    overflow: hidden;
    display: -webkit-box;
    -webkit-box-orient: vertical;
  }
  .lg p {
    font-size: 16px;
  }
</style>
