<script lang="ts" generics="T">
  // Virtualized vertical list. The focused row glides toward the middle of the view,
  // so there is always context above and below it. Only rows in view are in the DOM.
  import { untrack, type Snippet } from 'svelte'

  interface Props {
    items: T[]
    index: number
    rowHeight?: number
    /** Rows that fit in the view (reported for paging). */
    rows?: number
    row: Snippet<[T, number, boolean]>
    onactivate?: (i: number) => void
  }

  let { items, index = $bindable(0), rowHeight = 46, rows = $bindable(1), row, onactivate }: Props = $props()

  let height = $state(0)
  let offset = $state(0)
  let target = 0
  let raf = 0

  // Space above the first row and below the last, clear of the faded edges.
  const PAD = 30
  const total = $derived(items.length * rowHeight + 2 * PAD)
  $effect(() => {
    rows = Math.max(1, Math.floor(height / rowHeight))
  })

  $effect(() => {
    const want = Math.min(Math.max(PAD + index * rowHeight - height / 2 + rowHeight / 2, 0), Math.max(total - height, 0))
    untrack(() => {
      target = want
      animate()
    })
  })

  function animate() {
    if (raf) return
    const tick = () => {
      const d = target - offset
      if (Math.abs(d) < 0.5) {
        offset = target
        raf = 0
        return
      }
      offset += d * 0.3
      raf = requestAnimationFrame(tick)
    }
    raf = requestAnimationFrame(tick)
  }

  const first = $derived(Math.max(0, Math.floor((offset - PAD) / rowHeight) - 2))
  const last = $derived(Math.min(items.length - 1, Math.ceil((offset + height - PAD) / rowHeight) + 2))
  const visible = $derived.by(() => {
    const out: number[] = []
    for (let i = first; i <= last; i++) out.push(i)
    return out
  })

  function wheel(e: WheelEvent) {
    e.preventDefault()
    target = Math.min(Math.max(target + e.deltaY, 0), Math.max(total - height, 0))
    animate()
  }
</script>

<div class="viewport" role="listbox" tabindex="-1" bind:clientHeight={height} onwheel={wheel}>
  <div class="inner" style:transform="translate3d(0, {-offset}px, 0)">
    {#each visible as i (i)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        class="row"
        role="option"
        tabindex="-1"
        aria-selected={i === index}
        style:height="{rowHeight}px"
        style:transform="translate3d(0, {PAD + i * rowHeight}px, 0)"
        onclick={() => (index === i ? onactivate?.(i) : (index = i))}
      >
        {@render row(items[i], i, i === index)}
      </div>
    {/each}
  </div>
</div>

<style>
  .viewport {
    position: relative;
    overflow: hidden;
    height: 100%;
    /* Fade rows out at the edges instead of cutting them. */
    mask-image: linear-gradient(to bottom, transparent 0, #000 28px, #000 calc(100% - 28px), transparent 100%);
  }
  .inner {
    position: absolute;
    inset: 0;
    will-change: transform;
  }
  .row {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
  }
</style>
