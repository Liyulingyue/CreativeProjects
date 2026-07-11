import { useEffect, useState } from 'react'
import { loadVisited } from '../lib/storage'

export function useVisitedVenues() {
  const [visited, setVisited] = useState<Set<string>>(loadVisited())
  const [version, setVersion] = useState(0)

  useEffect(() => {
    function onStorage() {
      setVisited(loadVisited())
      setVersion((v) => v + 1)
    }
    // Listen for cross-tab + same-tab updates
    window.addEventListener('storage', onStorage)
    // Same-tab updates (storage event doesn't fire on the originating tab)
    const interval = setInterval(onStorage, 1500) // poll as fallback

    // Also a custom event for explicit notification
    function onCustom() {
      onStorage()
    }
    window.addEventListener('zooguide:visitedChanged', onCustom)

    return () => {
      window.removeEventListener('storage', onStorage)
      window.removeEventListener('zooguide:visitedChanged', onCustom)
      clearInterval(interval)
    }
  }, [])

  return { visited, version }
}

export function notifyVisitedChanged() {
  // Trigger immediate refresh in other components
  window.dispatchEvent(new Event('zooguide:visitedChanged'))
}