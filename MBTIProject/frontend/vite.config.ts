import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:8000',
    },
    // 系统级 inotify watcher 耗尽时，可用 VITE_POLLING=1 降级为轮询监听
    watch: process.env.VITE_POLLING ? { usePolling: true, interval: 400 } : undefined,
  },
})
