<script lang="ts">
  // A yes/no question on top of the screen. A confirms, B cancels; nothing else gets
  // through while it is open.
  import { onDestroy } from 'svelte'
  import { onAction } from '../lib/input'

  let {
    title,
    message,
    confirm = 'Confirm',
    onconfirm,
    oncancel,
  }: { title: string; message: string; confirm?: string; onconfirm: () => void; oncancel: () => void } = $props()

  const off = onAction(({ action }) => {
    if (action === 'a') onconfirm()
    else if (action === 'b') oncancel()
    return true
  })
  onDestroy(off)
</script>

<div class="scrim">
  <div class="box" role="alertdialog" aria-labelledby="confirm-title">
    <h2 id="confirm-title">{title}</h2>
    <p>{message}</p>
    <div class="buttons">
      <button class="yes" tabindex="-1" onclick={onconfirm}><span class="btn a">A</span>{confirm}</button>
      <button class="no" tabindex="-1" onclick={oncancel}><span class="btn b">B</span>Cancel</button>
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: rgb(0 0 0 / 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    animation: fade 140ms ease-out;
  }
  .box {
    width: min(520px, 90vw);
    padding: 28px 30px 24px;
    border-radius: 18px;
    background: var(--surface-2);
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.6);
  }
  h2 {
    margin: 0 0 10px;
    font-size: 22px;
  }
  p {
    margin: 0 0 22px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .buttons {
    display: flex;
    gap: 14px;
    justify-content: flex-end;
  }
  button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 18px;
    border: 0;
    border-radius: 10px;
    font: inherit;
    font-weight: 600;
    color: var(--text);
    background: rgb(255 255 255 / 0.08);
    cursor: pointer;
  }
  .yes {
    background: var(--danger);
    color: #fff;
  }
  .btn {
    width: 22px;
    height: 22px;
    border-radius: 11px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 800;
    background: rgb(0 0 0 / 0.3);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
</style>
