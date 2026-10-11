import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { readFileSync, existsSync } from 'node:fs'
import { resolve } from 'node:path'

// TLS for the dev server. Service workers only register on a secure context,
// and "secure context" means a *trusted* certificate -- a bare self-signed cert
// that the phone has not been taught to trust still gets blocked, so the CA in
// ../certs must be installed on the device. The certs are gitignored.
const certDir = resolve(__dirname, '../certs')
const hasCerts = existsSync(resolve(certDir, 'server.crt')) && existsSync(resolve(certDir, 'server.key'))

export default defineConfig({
  plugins: [react()],
  server: {
    host: '0.0.0.0',
    port: 15173,
    https: hasCerts
      ? {
          key: readFileSync(resolve(certDir, 'server.key')),
          cert: readFileSync(resolve(certDir, 'server.crt')),
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