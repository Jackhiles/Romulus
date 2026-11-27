<script lang="ts" generics="T">
  // Virtualized grid: only rows in view (plus one above and below) are in the DOM. The
  // view follows the focused index with an eased scroll. Navigation itself is done by
  // the screen (see lib/grid.ts); this component reports `cols` and `rows` so it can.
  import { untrack, type Snippet } from 'svelte'
  import { follow } from '../lib/ease'
  import { scrollFor } from '../lib/grid'

  interface Props {
    items: T[]
    index: number
    tileWidth: number
    tileHeight: number
    gap?: number
    cols?: number
    rows?: number
    tile: Snippet<[T, number, boolean]>
    onactivate?: (i: number) => void
  }

  let {
    items,
    index = $bindable(0),
    tileWidth,
    tileHeight,
    gap = 20,
    cols = $bindable(1),
    rows = $bindable(1),
    tile,
    onactivate,
  }: Props = $props()

  let width = $state(0)
  let height = $state(0)
  let offset = $state(0)
  let target = $state(0)
  let raf = 0

  const rowH = $derived(tileHeight + gap)
  $effect(() => {
    cols = Math.max(1, Math.floor((width + gap) / (tileWidth + gap)))
    rows = Math.max(1, Math.floor(height / rowH))
  })
  const totalRows = $derived(Math.ceil(items.length / cols))
  const totalH = $derived(totalRows * rowH + gap)
  // Center the columns.
  const sidePad = $derived(Math.max(0, (width - (cols * (tileWidth + gap) - gap)) / 2))

  // Follow focus.
  $effect(() => {
    const row = Math.floor(index / cols)
    const next = scrollFor(row, rowH, height, untrack(() => target), totalH)
    untrack(() => {
      target = next
      animate()
    })
  })

  function animate() {
    if (raf) return
    let last = performance.now()
    const tick = (now: number) => {
      offset = follow(offset, target, now - last)
      last = now
      raf = offset === target ? 0 : requestAnimationFrame(tick)
    }
    raf = requestAnimationFrame(tick)
  }

  const first = $derived(Math.max(0, Math.floor(offset / rowH) - 1))
  const last = $derived(Math.min(totalRows - 1, Math.ceil((offset + height) / rowH) + 1))
  const visible = $derived.by(() => {
    const out: number[] = []
    for (let r = first; r <= last; r++) {
      for (let c = 0; c < cols; c++) {
        const i = r * cols + c
        if (i < items.length) out.push(i)
      }
    }
    return out
  })

  function wheel(e: WheelEvent) {
    e.preventDefault()
    target = Math.min(Math.max(target + e.deltaY, 0), Math.max(totalH - height, 0))
    animate()
  }

  let touchY: number | null = null
  function touchstart(e: TouchEvent) {
    touchY = e.touches[0].clientY
  }
  function touchmove(e: TouchEvent) {
    if (touchY === null) return
    const y = e.touches[0].clientY
    target = Math.min(Math.max(target + (touchY - y), 0), Math.max(totalH - height, 0))
    offset = target
    touchY = y
  }
</script>

<div
  class="viewport"
  role="grid"
  tabindex="-1"
  bind:clientWidth={width}
  bind:clientHeight={height}
  onwheel={wheel}
  ontouchstart={touchstart}
  ontouchmove={touchmove}
  ontouchend={() => (touchY = null)}
>
  <div class="inner" style:transform="translate3d(0, {-offset}px, 0)">
    {#each visible as i (i)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div
        class="cell"
        role="button"
        tabindex="-1"
        style:width="{tileWidth}px"
        style:height="{tileHeight}px"
        style:transform="translate3d({sidePad + (i % cols) * (tileWidth + gap)}px, {gap / 2 + Math.floor(i / cols) * rowH}px, 0)"
        onclick={() => (index === i ? onactivate?.(i) : (index = i))}
      >
        {@render tile(items[i], i, i === index)}
      </div>
    {/each}
  </div>
</div>

<style>
  .viewport {
    position: relative;
    overflow: hidden;
    width: 100%;
    height: 100%;
    touch-action: none;
  }
  .inner {
    position: absolute;
    inset: 0;
    will-change: transform;
  }
  .cell {
    position: absolute;
    top: 0;
    left: 0;
  }
</style>
