import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      // @crow-dev/ui's package.json `exports` map only exposes ".", so a
      // deep import of its stylesheet doesn't resolve under Vite/Rolldown.
      // This aliases a clean specifier to the real file on disk instead.
      '@crow-dev/ui/style.css': fileURLToPath(
        new URL('./node_modules/@crow-dev/ui/dist/style.css', import.meta.url),
      ),
    },
  },
  server: {
    proxy: {
      '/api': 'http://localhost:8080',
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
  },
})
