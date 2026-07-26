import { useEffect, useState } from 'react'
import { loadPhotoLogBySource, removePhotoLogEntry, type PhotoLogEntry } from '../../lib/storage'

interface Props {
  onClose: () => void
  onOpenPhoto: () => void
}

export function PhotoWallFlow({ onClose, onOpenPhoto }: Props) {
  const [log, setLog] = useState<PhotoLogEntry[]>(loadPhotoLogBySource('wall'))
  const [filter, setFilter] = useState<'all' | 'high' | 'today'>('all')
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null)
  const [previewUrl, setPreviewUrl] = useState<string | null>(null)

  useEffect(() => {
    function refresh() {
      setLog(loadPhotoLogBySource('wall'))
    }
    window.addEventListener('zooguide:photoLogChanged', refresh)
    return () => window.removeEventListener('zooguide:photoLogChanged', refresh)
  }, [])

  const today = new Date().toDateString()
  const filtered = log.filter((p) => {
    if (filter === 'high') return p.score >= 80
    if (filter === 'today') return new Date(p.ts).toDateString() === today
    return true
  })

  const maxScore = log.length > 0 ? Math.max(...log.map((p) => p.score)) : 0
  const avgScore =
    log.length > 0 ? Math.round(log.reduce((s, p) => s + p.score, 0) / log.length) : 0

  function handleDelete(evaluationId: string) {
    removePhotoLogEntry(evaluationId)
    setConfirmDeleteId(null)
  }

  const blurryLabel: Record<string, string> = {
    '清晰': '✓',
    '略微模糊': '◐',
    '模糊': '✗',
  }

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={onClose}>
          ←
        </button>
        <div className="flow-title">🌟 出片墙</div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: '1fr 1fr 1fr',
            gap: 8,
            marginBottom: 14,
          }}
        >
          <StatBox label="总张数" value={log.length} />
          <StatBox label="最高分" value={maxScore} />
          <StatBox label="平均分" value={avgScore} />
        </div>
        <button
          className="btn btn-primary btn-full"
          style={{ marginBottom: 14 }}
          onClick={onOpenPhoto}
        >
          📷 拍照评分
        </button>

        <div
          style={{
            display: 'flex',
            gap: 6,
            marginBottom: 14,
          }}
        >
          {[
            { key: 'all', label: '全部' },
            { key: 'high', label: '高分 80+' },
            { key: 'today', label: '今天' },
          ].map((f) => (
            <button
              key={f.key}
              className="chat-quick-chip"
              style={
                filter === f.key
                  ? { background: 'var(--primary)', color: 'white', borderColor: 'var(--primary)' }
                  : undefined
              }
              onClick={() => setFilter(f.key as any)}
            >
              {f.label}
            </button>
          ))}
        </div>

        {filtered.length === 0 ? (
          <div
            className="card"
            style={{
              textAlign: 'center',
              color: 'var(--fg-muted)',
              padding: 40,
            }}
          >
            <div style={{ fontSize: 48, marginBottom: 8 }}>📷</div>
            还没有出片
            <div style={{ fontSize: 12, marginTop: 6 }}>
              {filter === 'today' ? '今天还没拍' : filter === 'high' ? '还没有 80+ 分的出片' : '去拍第一张'}
            </div>
            <button
              className="btn btn-primary"
              style={{ marginTop: 14 }}
              onClick={onOpenPhoto}
            >
              📷 来一张
            </button>
          </div>
        ) : (
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr',
              gap: 10,
            }}
          >
            {filtered.map((p) => (
              <div key={p.evaluation_id} className="wall-photo-card" style={{ position: 'relative' }}>
                <button
                  onClick={() => setConfirmDeleteId(p.evaluation_id)}
                  style={{
                    position: 'absolute',
                    top: 6,
                    right: 6,
                    background: 'rgba(0,0,0,0.45)',
                    color: '#fff',
                    border: 'none',
                    borderRadius: '50%',
                    width: 22,
                    height: 22,
                    fontSize: 12,
                    cursor: 'pointer',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    lineHeight: 1,
                    zIndex: 1,
                  }}
                >
                  ×
                </button>
                {p.thumbnail ? (
                  <img
                    src={p.thumbnail}
                    alt={p.animal}
                    onClick={() => setPreviewUrl(p.preview || p.thumbnail || null)}
                    style={{
                      width: '100%',
                      aspectRatio: '1',
                      objectFit: 'cover',
                      borderRadius: 8,
                      marginBottom: 6,
                      cursor: 'pointer',
                    }}
                  />
                ) : (
                  <div
                    style={{
                      width: '100%',
                      aspectRatio: '1',
                      background: 'var(--bg-elev)',
                      borderRadius: 8,
                      marginBottom: 6,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      fontSize: 32,
                    }}
                  >
                    📷
                  </div>
                )}
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                  <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--fg)' }}>
                    {p.animal || '未知动物'}
                  </div>
                  <div
                    style={{
                      background: p.score >= 80 ? '#10b981' : p.score >= 50 ? '#f59e0b' : 'var(--fg-muted)',
                      color: '#fff',
                      borderRadius: 6,
                      padding: '2px 8px',
                      fontSize: 12,
                      fontWeight: 700,
                    }}
                  >
                    {p.score}
                  </div>
                </div>
                {p.desc && (
                  <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginTop: 4, lineHeight: 1.4 }}>
                    {p.desc}
                  </div>
                )}
                <div style={{ display: 'flex', gap: 6, marginTop: 4, flexWrap: 'wrap' }}>
                  {p.style && p.style !== '其他' && (
                    <span style={{ fontSize: 11, background: 'var(--primary-soft)', color: 'var(--primary-strong)', padding: '1px 6px', borderRadius: 4 }}>
                      {p.style}
                    </span>
                  )}
                  {p.blurry && (
                    <span style={{ fontSize: 11, background: p.blurry === '清晰' ? '#d1fae5' : p.blurry === '略微模糊' ? '#fef3c7' : '#fee2e2', color: 'var(--fg-muted)', padding: '1px 6px', borderRadius: 4 }}>
                      {blurryLabel[p.blurry] || ''} {p.blurry}
                    </span>
                  )}
                </div>
                <div style={{ fontSize: 11, color: 'var(--fg-muted)', marginTop: 4 }}>
                  {new Date(p.ts).toLocaleString('zh-CN', {
                    month: 'numeric',
                    day: 'numeric',
                    hour: '2-digit',
                    minute: '2-digit',
                  })}
                </div>
              </div>
            ))}
          </div>
        )}

        {confirmDeleteId && (
          <div className="modal-mask" onClick={() => setConfirmDeleteId(null)} role="presentation">
            <div className="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
              <h3>删除这条出片？</h3>
              <p style={{ fontSize: 13, color: 'var(--fg-muted)', margin: '0 0 16px' }}>
                删除后不可恢复
              </p>
              <div className="modal-actions">
                <button className="btn btn-ghost" onClick={() => setConfirmDeleteId(null)}>
                  取消
                </button>
                <button className="btn btn-primary" onClick={() => handleDelete(confirmDeleteId)}>
                  删除
                </button>
              </div>
            </div>
          </div>
        )}
      </div>

      {previewUrl && (
        <div
          className="modal-mask"
          onClick={() => setPreviewUrl(null)}
          style={{ background: 'rgba(0,0,0,0.9)', zIndex: 1000 }}
          role="presentation"
        >
          <img
            src={previewUrl}
            alt="预览"
            onClick={(e) => e.stopPropagation()}
            style={{
              maxWidth: '90vw',
              maxHeight: '85vh',
              borderRadius: 12,
              objectFit: 'contain',
            }}
          />
          <button
            onClick={() => setPreviewUrl(null)}
            style={{
              position: 'absolute',
              top: 16,
              right: 16,
              background: 'rgba(255,255,255,0.2)',
              color: '#fff',
              border: 'none',
              borderRadius: '50%',
              width: 36,
              height: 36,
              fontSize: 18,
              cursor: 'pointer',
            }}
          >
            ×
          </button>
        </div>
      )}
    </div>
  )
}

function StatBox({ label, value }: { label: string; value: number }) {
  return (
    <div className="activity-stat-cell">
      <div className="activity-stat-num">{isNaN(value) ? '-' : value}</div>
      <div className="activity-stat-label">{label}</div>
    </div>
  )
}
