import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

// Tauri expects a fixed dev port and leaves the terminal output alone.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    // WebKitGTK on SteamOS is roughly Safari 16-level.
    target: 'safari16',
    outDir: 'dist',
    emptyOutDir: true,
  },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
})
