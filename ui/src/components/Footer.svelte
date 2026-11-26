<script lang="ts">
  // Button hints for the current screen. Labels follow the Deck's Xbox-style layout.
  import type { Action } from '../lib/input'
  import { dispatch } from '../lib/input'

  let { hints }: { hints: [Action, string][] } = $props()

  const LABEL: Record<Action, string> = {
    a: 'A', b: 'B', x: 'X', y: 'Y', lb: 'LB', rb: 'RB', lt: 'LT', rt: 'RT',
    start: '☰', select: '⧉', up: '↑', down: '↓', left: '←', right: '→',
  }
</script>

<footer>
  {#each hints as [action, label] (action)}
    <button class="hint" tabindex="-1" onclick={() => dispatch({ action, repeat: false })}>
      <span class="btn btn-{action}">{LABEL[action]}</span>
      <span>{label}</span>
    </button>
  {/each}
</footer>

<style>
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 22px;
    padding: 10px 28px;
    background: linear-gradient(to top, rgb(0 0 0 / 0.5), transparent);
    font-size: 15px;
    color: var(--muted);
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 8px;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    padding: 0;
    cursor: pointer;
  }
  .btn {
    min-width: 26px;
    height: 26px;
    padding: 0 6px;
    border-radius: 13px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    font-weight: 700;
    background: #3a3f4b;
    color: #fff;
  }
  .btn-a { background: #3d8b40; }
  .btn-b { background: #c0392b; }
  .btn-x { background: #2e6fb7; }
  .btn-y { background: #c19a1c; }
  .btn-lb, .btn-rb, .btn-lt, .btn-rt { border-radius: 6px; }
</style>
