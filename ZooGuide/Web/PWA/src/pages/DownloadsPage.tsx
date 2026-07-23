import { useNavigate } from 'react-router-dom'
import type { Meta } from '../types'

interface Props {
  meta: Meta | null
}

export function DownloadsPage({ meta }: Props) {
  const navigate = useNavigate()
  const downloads = meta?.downloads || []

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={() => navigate('/')}>←</button>
        <div className="flow-title">📥 资料下载</div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        {downloads.length === 0 ? (
          <div className="card" style={{ textAlign: 'center', color: 'var(--fg-muted)', padding: 30 }}>
            加载中…
          </div>
        ) : (
          <div className="downloads-list">
            {downloads.map((d) => (
              <a
                key={d.id}
                className="download-card"
                href={`/downloads/${d.file}`}
                download
                target="_blank"
                rel="noopener noreferrer"
              >
                <div className="dc-icon">{d.icon}</div>
                <div className="dc-body">
                  <div className="dc-title">{d.title}</div>
                  <div className="dc-desc">{d.desc}</div>
                </div>
                <div className="dc-meta">
                  <span className="dc-tag">{d.tag}</span>
                  <span className="dc-arrow">⬇</span>
                </div>
              </a>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
