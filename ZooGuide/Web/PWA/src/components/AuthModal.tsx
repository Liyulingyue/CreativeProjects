import { useState } from 'react'
import { api } from '../api/client'
import { setAuth } from '../lib/storage'

interface Props {
  onClose: () => void
  onAuthed: (user: { id: number; username: string; display_name: string }) => void
}

type Mode = 'login' | 'register'

export function AuthModal({ onClose, onAuthed }: Props) {
  const [mode, setMode] = useState<Mode>('login')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function submit() {
    setLoading(true)
    setError(null)
    try {
      const result =
        mode === 'login'
          ? await api.login(username, password)
          : await api.register(username, password, displayName || undefined)
      setAuth(result.token, result.user)
      onAuthed(result.user)
      onClose()
    } catch (e) {
      setError(e instanceof Error ? e.message : '失败')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 400 }}>
        <h3>{mode === 'login' ? '🔑 登录' : '✨ 注册'}</h3>
        <p style={{ fontSize: 12, color: 'var(--fg-muted)', margin: '0 0 14px' }}>
          {mode === 'login'
            ? '登录后可以在多个设备看到你的打卡、照片、路线历史'
            : '只需用户名+密码，注册即用'}
        </p>

        <div style={{ marginBottom: 10 }}>
          <input
            type="text"
            placeholder="用户名（2-32字符）"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
            style={inputStyle}
          />
        </div>
        <div style={{ marginBottom: 10 }}>
          <input
            type="password"
            placeholder="密码（≥4位）"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete={mode === 'login' ? 'current-password' : 'new-password'}
            style={inputStyle}
            onKeyDown={(e) => e.key === 'Enter' && submit()}
          />
        </div>
        {mode === 'register' && (
          <div style={{ marginBottom: 10 }}>
            <input
              type="text"
              placeholder="昵称（可选）"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              style={inputStyle}
            />
          </div>
        )}

        {error && <div className="error-banner" style={{ marginTop: 8 }}>{error}</div>}

        <div className="modal-actions">
          <button
            className="btn btn-ghost"
            onClick={() => {
              setMode(mode === 'login' ? 'register' : 'login')
              setError(null)
            }}
          >
            {mode === 'login' ? '没账号？注册' : '已有账号？登录'}
          </button>
          <button
            className="btn btn-primary"
            onClick={submit}
            disabled={loading || username.length < 2 || password.length < 4}
          >
            {loading ? '处理中…' : mode === 'login' ? '登录' : '注册并登录'}
          </button>
        </div>
      </div>
    </div>
  )
}

const inputStyle: React.CSSProperties = {
  width: '100%',
  padding: '10px 14px',
  border: '1px solid var(--border)',
  borderRadius: 10,
  background: '#fff',
  fontSize: 15,
  color: 'var(--fg)',
}