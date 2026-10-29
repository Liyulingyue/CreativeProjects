import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import { ToastProvider } from './components/ui/Toast'
import './index.css'

// Service workers only register on a secure context, so this silently does
// nothing on a plain-http LAN address -- that is expected, not a failure. It
// starts working once the app is served over https.
//
// Not in development, though. The worker caches what it fetches, and the dev
// server serves modules that are not content-hashed, so a returning developer
// would keep running the first version of every file they ever loaded --
// hot reload silently doing nothing, which is a miserable way to spend an
// afternoon.
if (import.meta.env.PROD && 'serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch(() => {
      // Offline shell is a nicety; failing to register must never break the app.
    })
  })
}

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ToastProvider>
      <App />
    </ToastProvider>
  </React.StrictMode>,
)
