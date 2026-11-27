// Controller-first input. Actions come from three sources and are routed to the screen
// on top of the stack:
//  - `pad` events from Rust (gilrs; the default, see src-tauri/src/input.rs)
//  - the browser Gamepad API (fallback, behind a setting)
//  - the keyboard (development, and Steam's keyboard-and-mouse layouts)
import { listen } from '@tauri-apps/api/event'
import { noteInput } from './perf'

export type Action =
  | 'up' | 'down' | 'left' | 'right'
  | 'a' | 'b' | 'x' | 'y'
  | 'lb' | 'rb' | 'lt' | 'rt'
  | 'start' | 'select'

export interface PadEvent {
  action: Action
  repeat: boolean
}

type Handler = (e: PadEvent) => boolean | void

const handlers: Handler[] = []

/** Register a handler. The most recent one gets actions first; return true to stop
 *  older handlers from seeing the action. Returns an unregister function. */
export function onAction(h: Handler): () => void {
  handlers.push(h)
  return () => {
    const i = handlers.lastIndexOf(h)
    if (i >= 0) handlers.splice(i, 1)
  }
}

let lastInput = 0

export function dispatch(e: PadEvent) {
  lastInput = performance.now()
  noteInput()
  for (let i = handlers.length - 1; i >= 0; i--) {
    if (handlers[i](e) === true) return
  }
}

/** Milliseconds since the last controller or keyboard action. */
export function idleMs(): number {
  return performance.now() - lastInput
}

const KEYS: Record<string, Action> = {
  ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right',
  Enter: 'a', Escape: 'b', Backspace: 'b',
  x: 'x', y: 'y', q: 'lb', e: 'rb', '[': 'lt', ']': 'rt',
  Tab: 'select', F1: 'start',
}

function isTextField(t: EventTarget | null): t is HTMLInputElement {
  return t instanceof HTMLInputElement && (t.type === 'text' || t.type === 'password' || t.type === 'search' || t.type === 'url')
}

export function startInput() {
  listen<PadEvent>('pad', (e) => dispatch(e.payload)).catch(() => {
    // Not running inside Tauri (plain browser during UI development).
  })

  window.addEventListener('keydown', (ev) => {
    // In a text field, only navigation keys leave it; typing stays typing.
    if (isTextField(ev.target)) {
      if (!['ArrowUp', 'ArrowDown', 'Enter', 'Escape', 'Tab'].includes(ev.key)) return
    }
    const action = KEYS[ev.key]
    if (!action) return
    ev.preventDefault()
    dispatch({ action, repeat: ev.repeat })
  })
}

// ---- browser Gamepad API fallback ------------------------------------------------------

const BUTTONS: [number, Action][] = [
  [0, 'a'], [1, 'b'], [2, 'x'], [3, 'y'], [4, 'lb'], [5, 'rb'], [6, 'lt'], [7, 'rt'],
  [8, 'select'], [9, 'start'], [12, 'up'], [13, 'down'], [14, 'left'], [15, 'right'],
]
let webPadOn = false

export function setWebGamepad(on: boolean) {
  if (on === webPadOn) return
  webPadOn = on
  if (!on) return
  const held = new Map<Action, { since: number; last: number }>()
  const tick = (now: number) => {
    if (!webPadOn) return
    const pad = navigator.getGamepads?.().find((p) => p && p.connected)
    const down = new Set<Action>()
    if (pad) {
      for (const [i, a] of BUTTONS) if (pad.buttons[i]?.pressed) down.add(a)
      const [x = 0, y = 0] = pad.axes
      if (y < -0.55) down.add('up')
      if (y > 0.55) down.add('down')
      if (x < -0.55) down.add('left')
      if (x > 0.55) down.add('right')
    }
    for (const a of [...held.keys()]) if (!down.has(a)) held.delete(a)
    for (const a of down) {
      const h = held.get(a)
      if (!h) {
        held.set(a, { since: now, last: now })
        dispatch({ action: a, repeat: false })
      } else if (['up', 'down', 'left', 'right', 'lb', 'rb'].includes(a) && now - h.since > 320 && now - h.last > 70) {
        h.last = now
        dispatch({ action: a, repeat: true })
      }
    }
    requestAnimationFrame(tick)
  }
  requestAnimationFrame(tick)
}
