import { useEffect, useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import type { Venue } from '../types'
import { api } from '../api/client'
import { PhotoFlow } from '../components/flows/PhotoFlow'
import { loadVisited, saveVisited } from '../lib/storage'

export function PhotoFlowPage() {
  const { venueId } = useParams<{ venueId: string }>()
  const navigate = useNavigate()
  const [venue, setVenue] = useState<Venue | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    api
      .venues()
      .then((d) => {
        const v = d.venues.find((v: Venue) => v.id === venueId)
        if (v) setVenue(v)
      })
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [venueId])

  if (loading) {
    return (
      <div className="loading">
        <div className="spinner" />
      </div>
    )
  }

  if (!venue) {
    return (
      <div className="loading">
        <p>场馆不存在</p>
        <button className="btn btn-ghost" onClick={() => navigate('/activity')}>
          返回
        </button>
      </div>
    )
  }

  return (
    <PhotoFlow
      venue={venue}
      onClose={() => navigate('/activity')}
      onCheckinSuccess={(id) => {
        const next = new Set(loadVisited())
        next.add(id)
        saveVisited(next)
      }}
    />
  )
}
