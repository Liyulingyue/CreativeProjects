import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { readFileSync, existsSync } from 'node:fs'
import { resolve } from 'node:path'

// Local self-signed pair for the dev server, so the PWA shell can be exercised
// over https on a LAN address. Generated per-machine by scripts/dev-cert.sh and
// gitignored; it is not trusted by any browser, it only makes the origin
// secure enough to register a service worker locally.
const certDir = resolve(__dirname)
const hasCerts = existsSync(resolve(certDir, 'dev-cert.pem')) && existsSync(resolve(certDir, 'dev-key.pem'))

export default defineConfig({
  plugins: [react()],
  server: {
    host: '0.0.0.0',
    port: 15173,
    https: hasCerts
      ? {
          key: readFileSync(resolve(certDir, 'dev-key.pem')),
          cert: readFileSync(resolve(certDir, 'dev-cert.pem')),
        }
      : undefined,
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