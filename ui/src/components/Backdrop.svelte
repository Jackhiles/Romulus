<script lang="ts">
  // Full-screen background: a tint for the current system plus, optionally, a dimmed
  // screenshot. Changes crossfade (opacity only; blur is too slow in WebKitGTK), and a
  // gradient keeps text readable on top.
  import { untrack } from 'svelte'

  let { src = null, hue = 220, strength = 0.3 }: { src?: string | null; hue?: number; strength?: number } = $props()

  let layers = $state<{ key: number; src: string | null; hue: number }[]>([])
  let next = 0

  $effect(() => {
    const s = src
    const h = hue
    untrack(() => {
      const top = layers[layers.length - 1]
      if (top && top.src === s && top.hue === h) return
      layers = [...layers.slice(-1), { key: next++, src: s, hue: h }]
    })
  })
</script>

<div class="backdrop" style:--strength={strength}>
  {#each layers as l (l.key)}
    <div class="layer" style:--hue={l.hue}>
      {#if l.src}<div class="img" style:background-image={`url("${l.src}")`}></div>{/if}
    </div>
  {/each}
  <div class="shade"></div>
</div>

<style>
  .backdrop {
    /* Only the top screen is mounted, so one backdrop covers the whole window. */
    position: fixed;
    inset: 0;
    z-index: 0;
    overflow: hidden;
    background: var(--bg);
  }
  .layer {
    position: absolute;
    inset: 0;
    background: radial-gradient(ellipse 90% 70% at 75% 0%, hsl(var(--hue) 50% 18%), transparent 70%),
      radial-gradient(ellipse 70% 60% at 0% 100%, hsl(var(--hue) 40% 10%), transparent 70%), var(--bg);
    animation: fade 280ms ease-out;
  }
  .img {
    position: absolute;
    inset: 0;
    background-size: cover;
    background-position: center;
    opacity: var(--strength);
  }
  .shade {
    position: absolute;
    inset: 0;
    background:
      linear-gradient(to right, rgb(11 13 18 / 0.92) 0%, rgb(11 13 18 / 0.6) 45%, rgb(11 13 18 / 0.25) 100%),
      linear-gradient(to top, rgb(11 13 18 / 0.95) 0%, transparent 35%);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
</style>
