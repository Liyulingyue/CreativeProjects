// Minimal offline shell for the PWA.
//
// Scope is deliberately tiny: precache the app shell so a cold start works
// without the network, and let everything else go straight to the network.
// An aggressive cache-first strategy would be actively harmful here, because
// every meaningful read (search results, file metadata, SSE chat streams) comes
// from /api and has to stay live -- the chunked stream endpoints in particular
// must never be served from a cache or the token-by-token output arrives in
// one lump, or breaks entirely.

const VERSION = 'sfm-shell-v1'
const SHELL = [
  '/',
  '/index.html',
  '/manifest.webmanifest',
  '/icon-192.png',
  '/icon-512.png',
  '/icon-maskable-512.png',
  '/apple-touch-icon.png',
]

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(VERSION)
      // Individually, so one missing icon cannot fail the whole install.
      .then((cache) => Promise.allSettled(SHELL.map((url) => cache.add(url))))
      .then(() => self.skipWaiting()),
  )
})

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== VERSION).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  )
})

self.addEventListener('fetch', (event) => {
  const { request } = event
  if (request.method !== 'GET') return

  const url = new URL(request.url)
  if (url.origin !== self.location.origin) return
  // API responses and SSE streams are always live; never touch the cache.
  if (url.pathname.startsWith('/api/')) return

  // Navigations: network first so a deploy is picked up immediately, falling
  // back to the cached shell when offline.
  if (request.mode === 'navigate') {
    event.respondWith(
      fetch(request)
        .then((response) => {
          const copy = response.clone()
          caches.open(VERSION).then((cache) => cache.put('/index.html', copy))
          return response
        })
        .catch(() => caches.match('/index.html')),
    )
    return
  }

  // Static assets: cache first, they are content-hashed by the build.
  event.respondWith(
    caches.match(request).then(
      (cached) =>
        cached ||
        fetch(request).then((response) => {
          if (response.ok && response.type === 'basic') {
            const copy = response.clone()
            caches.open(VERSION).then((cache) => cache.put(request, copy))
          }
          return response
        }),
    ),
  )
})