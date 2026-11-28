<script lang="ts">
  // A game's manual (a PDF from RomM), read with the controller: left/right or the
  // bumpers turn pages, LT/RT jump ten, A switches between the whole page and the page
  // filling the width (then up/down scroll). pdf.js draws each page onto a canvas.
  import { onDestroy } from 'svelte'
  import Footer from '../components/Footer.svelte'
  import Header from '../components/Header.svelte'
  import { assetUrl } from '../lib/api'
  import { onAction, type Action } from '../lib/input'
  import { clampPage, fitScale, type Fit } from '../lib/manual'

  let { title, path }: { title: string; path: string } = $props()

  // pdf.js types, without importing the library up front (it is loaded on demand).
  type Doc = import('pdfjs-dist').PDFDocumentProxy
  type Task = import('pdfjs-dist').RenderTask

  let doc: Doc | null = null
  let pages = $state(0)
  let page = $state(1)
  let fit = $state<Fit>('page')
  let error = $state<string | null>(null)
  let loading = $state(true)
  let canvas = $state<HTMLCanvasElement>()
  let viewW = $state(0)
  let viewH = $state(0)
  let pageH = $state(0)
  let scroll = $state(0)
  let task: Task | null = null
  let alive = true

  async function open() {
    try {
      const pdfjs = await import('pdfjs-dist/legacy/build/pdf.mjs')
      const worker = (await import('pdfjs-dist/legacy/build/pdf.worker.min.mjs?url')).default
      pdfjs.GlobalWorkerOptions.workerSrc = worker
      const url = await assetUrl(path, () => alive)
      if (!url) throw new Error('the manual could not be downloaded')
      const data = new Uint8Array(await (await fetch(url)).arrayBuffer())
      doc = await pdfjs.getDocument({ data, isEvalSupported: false }).promise
      if (!alive) return
      pages = doc.numPages
      loading = false
    } catch (e) {
      loading = false
      error = String(e instanceof Error ? e.message : e)
    }
  }
  open()

  // Draw the page whenever it, the fit or the screen size changes.
  $effect(() => {
    const n = page
    const f = fit
    const w = viewW
    const h = viewH
    const c = canvas
    // Read before the check below, so the effect runs again once the PDF is open.
    const total = pages
    if (!total || !doc || !c || !w || !h) return
    render(n, f, w, h, c).catch((e) => (error = String(e instanceof Error ? e.message : e)))
  })

  async function render(n: number, f: Fit, w: number, h: number, c: HTMLCanvasElement) {
    if (!doc) return
    task?.cancel()
    const p = await doc.getPage(n)
    if (n !== page || f !== fit) return
    const base = p.getViewport({ scale: 1 })
    const dpr = window.devicePixelRatio || 1
    const scale = fitScale(base.width, base.height, w, h, f)
    const vp = p.getViewport({ scale: scale * dpr })
    const off = document.createElement('canvas')
    off.width = Math.floor(vp.width)
    off.height = Math.floor(vp.height)
    task = p.render({ canvasContext: off.getContext('2d')!, viewport: vp })
    try {
      await task.promise
    } catch {
      return // cancelled by a newer page
    }
    if (n !== page || f !== fit) return
    // Swap in the finished page at once, so turning pages never shows a blank one.
    c.width = off.width
    c.height = off.height
    c.style.width = `${vp.width / dpr}px`
    c.style.height = `${vp.height / dpr}px`
    c.getContext('2d')!.drawImage(off, 0, 0)
    pageH = vp.height / dpr
    scroll = 0
  }

  function turn(delta: number) {
    const next = clampPage(page + delta, pages)
    if (next !== page) page = next
  }

  function scrollBy(dy: number) {
    scroll = Math.min(Math.max(scroll + dy, 0), Math.max(pageH - viewH, 0))
  }

  const off = onAction(({ action }) => {
    if (!pages) return false
    switch (action) {
      case 'left':
      case 'lb':
        turn(-1)
        return true
      case 'right':
      case 'rb':
        turn(1)
        return true
      case 'lt':
      case 'rt':
        turn(action === 'rt' ? 10 : -10)
        return true
      case 'up':
      case 'down':
        if (fit === 'width') {
          // Past the end of the page, go on to the next one.
          const before = scroll
          scrollBy((action === 'down' ? 1 : -1) * viewH * 0.4)
          if (scroll === before) turn(action === 'down' ? 1 : -1)
        } else turn(action === 'down' ? 1 : -1)
        return true
      case 'a':
        fit = fit === 'page' ? 'width' : 'page'
        return true
    }
    return false
  })
  onDestroy(() => {
    alive = false
    off()
    task?.cancel()
    doc?.destroy()
  })

  const hints = $derived<[Action, string][]>([
    ['right', 'Page'],
    ['rt', '10 pages'],
    ['a', fit === 'page' ? 'Fit width' : 'Whole page'],
    ...(fit === 'width' ? ([['down', 'Scroll']] as [Action, string][]) : []),
    ['b', 'Back'],
  ])
</script>

<Header {title} subtitle={pages ? `Manual · page ${page} of ${pages}` : 'Manual'} />
<main bind:clientWidth={viewW} bind:clientHeight={viewH}>
  {#if error}
    <p class="msg">Couldn't open the manual: {error}</p>
  {:else if loading}
    <p class="msg">Loading the manual…</p>
  {/if}
  <canvas bind:this={canvas} class:hidden={!pages} style:transform="translate3d(0, {-scroll}px, 0)"></canvas>
</main>
<Footer {hints} />

<style>
  main {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    background: rgb(0 0 0 / 0.35);
  }
  canvas {
    display: block;
    box-shadow: 0 8px 30px rgb(0 0 0 / 0.5);
    transition: transform 160ms ease-out;
  }
  .hidden {
    opacity: 0;
  }
  .msg {
    position: absolute;
    top: 45%;
    color: var(--text-2);
    font-size: 18px;
  }
</style>
