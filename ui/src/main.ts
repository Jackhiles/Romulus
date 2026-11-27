import { invoke } from '@tauri-apps/api/core'
import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import { startInput } from './lib/input'

// Development only: run the UI in a browser against a fixture instead of the Rust
// backend (see src/dev/mockTauri.ts). Removed from production builds.
if (import.meta.env.DEV && import.meta.env.VITE_MOCK && !('__TAURI_INTERNALS__' in window)) {
  await import('./dev/mockTauri')
}

startInput()

// Send UI errors to the Rust log file (the webview has no console on the Deck).
function logUi(level: 'error' | 'warn', message: string) {
  invoke('log_ui', { level, message }).catch(() => {})
}
window.addEventListener('error', (e) => logUi('error', `${e.message} at ${e.filename}:${e.lineno}`))
window.addEventListener('unhandledrejection', (e) => logUi('error', `unhandled rejection: ${String(e.reason)}`))

// Right-click menus and text selection get in the way on a handheld.
window.addEventListener('contextmenu', (e) => e.preventDefault())

export default mount(App, { target: document.getElementById('app')! })
