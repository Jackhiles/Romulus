<script lang="ts">
  import { toasts } from '../lib/toast.svelte'
</script>

<div class="toasts">
  {#each toasts.list as t (t.id)}
    <div class="toast" class:error={t.kind === 'error'}>{t.text}</div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    top: 18px;
    right: 24px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    z-index: 100;
    pointer-events: none;
  }
  .toast {
    max-width: 420px;
    padding: 12px 16px;
    border-radius: 10px;
    background: var(--surface-2);
    border-left: 4px solid var(--accent);
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.45);
    font-size: 15px;
    animation: in 160ms ease-out;
  }
  .error {
    border-left-color: var(--danger);
  }
  @keyframes in {
    from { opacity: 0; transform: translateY(-8px); }
  }
</style>
