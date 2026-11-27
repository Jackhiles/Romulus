// Frame-time sampling while the user is moving around, so slowness on the Deck shows up
// in the log file as numbers. One summary line per burst of input, at most one per 10 s.
import { invoke } from '@tauri-apps/api/core'
import { idleMs } from './input'

let running = false
let frames: number[] = []
let lastLog = -Infinity

export function noteInput() {
  if (running) return
  running = true
  frames = []
  let prev = performance.now()
  const tick = (now: number) => {
    frames.push(now - prev)
    prev = now
    if (idleMs() < 600) {
      requestAnimationFrame(tick)
      return
    }
    running = false
    report(frames)
  }
  requestAnimationFrame(tick)
}

function report(f: number[]) {
  if (f.length < 30 || performance.now() - lastLog < 10_000) return
  lastLog = performance.now()
  const sorted = [...f].sort((a, b) => a - b)
  const avg = f.reduce((s, x) => s + x, 0) / f.length
  const p = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))]
  const slow = f.filter((x) => x > 33.4).length
  const msg = `frames: ${f.length}, avg ${avg.toFixed(1)} ms (${(1000 / avg).toFixed(0)} fps), p95 ${p(0.95).toFixed(1)} ms, max ${sorted[sorted.length - 1].toFixed(0)} ms, ${slow} over 33 ms`
  invoke('log_ui', { level: 'info', message: msg }).catch(() => {})
}
