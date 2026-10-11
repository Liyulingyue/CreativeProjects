import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    host: '0.0.0.0',
    port: 15173,
    proxy: {
      '/api': {
        target: 'http://localhost:8000',
        changeOrigin: true,
        // SSE must not be buffered or the agent stream arrives in one lump.
        configure: (proxy) => {
          proxy.on('proxyRes', (proxyRes) => {
            if (/\/stream$/.test(proxyRes.req.url ?? '')) {
              proxyRes.headers['x-accel-buffering'] = 'no'
            }
          })
        },
      },
    },
  },
})