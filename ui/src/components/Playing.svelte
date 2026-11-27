<script lang="ts">
  // Shown while a game runs. Swallows every action: the controller belongs to the game.
  import { onDestroy } from 'svelte'
  import { onAction } from '../lib/input'

  let { title }: { title: string } = $props()
  const off = onAction(() => true)
  onDestroy(off)
</script>

<div class="playing">
  <div class="dot"></div>
  <div class="label">Now playing</div>
  <h1>{title}</h1>
  <p>Romulus will be back when you quit the game.</p>
</div>

<style>
  .playing {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: var(--bg);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
    padding: 40px;
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    animation: pulse 1.6s ease-in-out infinite;
  }
  .label {
    color: var(--accent);
    font-size: 13px;
    font-weight: 800;
    letter-spacing: 0.18em;
    text-transform: uppercase;
  }
  h1 {
    margin: 0;
    font-size: 34px;
  }
  p {
    margin: 0;
    color: var(--muted);
  }
  @keyframes pulse {
    50% {
      opacity: 0.25;
    }
  }
</style>
