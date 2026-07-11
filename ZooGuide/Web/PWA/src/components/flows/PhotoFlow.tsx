import { useRef, useState } from 'react'
import { api } from '../../api/client'
import { loadPhotoLog, type PhotoLogEntry } from '../../lib/storage'

interface Props {
  onClose: () => void
}

type Phase = 'idle' | 'preview' | 'evaluating' | 'result' | 'error'

export function PhotoFlow({ onClose }: Props) {
  const [phase, setPhase] = useState<Phase>('idle')
  const [file, setFile] = useState<File | null>(null)
  const [previewUrl, setPreviewUrl] = useState<string | null>(null)
  const [evaluation, setEvaluation] = useState<any | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [usingCamera, setUsingCamera] = useState(false)
  const videoRef = useRef<HTMLVideoElement>(null)
  const streamRef = useRef<MediaStream | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const [history, setHistory] = useState<PhotoLogEntry[]>(loadPhotoLog())

  function pickFile(f: File) {
    if (f.size > 8 * 1024 * 1024) {
      setError('图片太大（最大 8MB）')
      return
    }
    if (previewUrl) URL.revokeObjectURL(previewUrl)
    setFile(f)
    setPreviewUrl(URL.createObjectURL(f))
    setPhase('preview')
    setError(null)
  }

  async function startCamera() {
    setUsingCamera(true)
    setError(null)
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: 'environment' },
      })
      streamRef.current = stream
      setTimeout(() => {
        if (videoRef.current) {
          videoRef.current.srcObject = stream
          videoRef.current.play().catch(() => {})
        }
      }, 50)
    } catch (e) {
      setError('相机权限被拒绝或不可用')
      setUsingCamera(false)
    }
  }

  function stopCamera() {
    if (streamRef.current) {
      streamRef.current.getTracks().forEach((t) => t.stop())
      streamRef.current = null
    }
    setUsingCamera(false)
  }

  function captureFromCamera() {
    if (!videoRef.current || !streamRef.current) return
    const video = videoRef.current
    const canvas = document.createElement('canvas')
    canvas.width = video.videoWidth || 640
    canvas.height = video.videoHeight || 480
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    ctx.drawImage(video, 0, 0, canvas.width, canvas.height)
    canvas.toBlob(
      (blob) => {
        if (!blob) return
        const f = new File([blob], `camera-${Date.now()}.jpg`, { type: 'image/jpeg' })
        pickFile(f)
        stopCamera()
      },
      'image/jpeg',
      0.85,
    )
  }

  async function submit() {
    if (!file) return
    setPhase('evaluating')
    setError(null)
    try {
      const result = await api.evaluatePhoto(file, file.name)
      setEvaluation(result)
      try {
        const { appendPhotoLog } = await import('../../lib/storage')
        appendPhotoLog({
          evaluation_id: result.evaluation_id,
          animal_guess: result.animal_guess,
          matched_venue_id: result.matched_venue_id,
          matched_venue_name: result.matched_venue_name,
          badge: result.badge,
          vibe_score: result.vibe_score,
          caption: result.caption,
          ts: result.ts,
        })
        setHistory(loadPhotoLog())
      } catch {}
      setPhase('result')
    } catch (e) {
      setError(e instanceof Error ? e.message : '评价失败')
      setPhase('error')
    }
  }

  function reset() {
    if (previewUrl) URL.revokeObjectURL(previewUrl)
    setFile(null)
    setPreviewUrl(null)
    setEvaluation(null)
    setError(null)
    setPhase('idle')
  }

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={onClose}>
          ←
        </button>
        <div className="flow-title">📷 拍照打卡</div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        {phase === 'idle' && !usingCamera && (
          <div className="flow-home">
            <div className="flow-hero">📷</div>
            <h2 style={{ margin: '12px 0 6px', color: 'var(--primary-strong)' }}>
              拍下你的红山一刻
            </h2>
            <p style={{ fontSize: 14, color: 'var(--fg-muted)', lineHeight: 1.6, margin: '0 0 18px' }}>
              拍动物照片，AI 识别 + 评分 + 自动打卡
            </p>
            <div style={{ display: 'flex', gap: 10 }}>
              <button
                className="btn btn-primary btn-full"
                onClick={() => fileInputRef.current?.click()}
              >
                📁 从相册选
              </button>
              <button
                className="btn btn-ghost btn-full"
                onClick={startCamera}
              >
                📷 拍一张
              </button>
            </div>
            <input
              ref={fileInputRef}
              type="file"
              accept="image/*"
              style={{ display: 'none' }}
              onChange={(e) => {
                const f = e.target.files?.[0]
                if (f) pickFile(f)
              }}
            />
          </div>
        )}

        {phase === 'idle' && usingCamera && (
          <>
            <video
              ref={videoRef}
              style={{
                width: '100%',
                borderRadius: 12,
                background: '#000',
                aspectRatio: 4 / 3,
                objectFit: 'cover',
              }}
              playsInline
              muted
            />
            <div className="modal-actions" style={{ marginTop: 12 }}>
              <button className="btn btn-ghost btn-full" onClick={stopCamera}>
                取消
              </button>
              <button className="btn btn-primary btn-full" onClick={captureFromCamera}>
                📸 拍下
              </button>
            </div>
          </>
        )}

        {phase === 'preview' && previewUrl && (
          <>
            <img
              src={previewUrl}
              alt="预览"
              style={{
                width: '100%',
                maxHeight: 360,
                objectFit: 'cover',
                borderRadius: 12,
                marginBottom: 12,
              }}
            />
            {error && <div className="error-banner" style={{ marginBottom: 10 }}>{error}</div>}
            <div className="modal-actions">
              <button className="btn btn-ghost btn-full" onClick={reset}>
                重选
              </button>
              <button className="btn btn-primary btn-full" onClick={submit}>
                ✨ 让 Agent 评分
              </button>
            </div>
          </>
        )}

        {phase === 'evaluating' && (
          <div className="loading">
            <div className="spinner" />
            正在分析照片…
            <div style={{ fontSize: 12, marginTop: 8 }}>AI 识别 + 自动打卡中</div>
          </div>
        )}

        {phase === 'result' && evaluation && (
          <ResultCard evaluation={evaluation} onAgain={reset} onClose={onClose} />
        )}

        {phase === 'error' && (
          <>
            <div className="error-banner">
              <strong>评价失败：</strong>
              {error}
            </div>
            <button className="btn btn-primary btn-full" onClick={() => setPhase('idle')}>
              再试一次
            </button>
          </>
        )}

        {phase === 'idle' && history.length > 0 && (
          <div style={{ marginTop: 28 }}>
            <div
              style={{
                fontSize: 12,
                fontWeight: 700,
                color: 'var(--primary-strong)',
                marginBottom: 8,
              }}
            >
              🖼 最近出片
            </div>
            <div className="activity-photos">
              {history.slice(0, 6).map((p) => (
                <div key={p.evaluation_id} className="activity-photo">
                  <div className="activity-photo-emoji">📷</div>
                  <div className="activity-photo-name">{p.matched_venue_name}</div>
                  <div className="activity-photo-badge">{p.badge}</div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

function ResultCard({
  evaluation,
  onAgain,
  onClose,
}: {
  evaluation: any
  onAgain: () => void
  onClose: () => void
}) {
  return (
    <div>
      {/* Auto checkin success banner */}
      {evaluation.auto_checkin && (
        <div
          style={{
            background: 'var(--primary)',
            color: 'white',
            padding: '10px 14px',
            borderRadius: 10,
            fontSize: 14,
            fontWeight: 600,
            marginBottom: 12,
            display: 'flex',
            alignItems: 'center',
            gap: 8,
          }}
        >
          <span style={{ fontSize: 18 }}>✓</span>
          <div>
            已自动打卡「{evaluation.auto_checkin.venue_name}」
            <div style={{ fontSize: 11, fontWeight: 400, opacity: 0.9, marginTop: 2 }}>
              在「我的」和「路线」里都能看到
            </div>
          </div>
        </div>
      )}

      {evaluation.new_achievements && evaluation.new_achievements.length > 0 && (
        <div
          style={{
            background: 'linear-gradient(135deg, #fbbf24, #f59e0b)',
            color: 'white',
            padding: '10px 14px',
            borderRadius: 10,
            fontSize: 14,
            fontWeight: 600,
            marginBottom: 12,
            display: 'flex',
            alignItems: 'center',
            gap: 8,
          }}
        >
          <span style={{ fontSize: 18 }}>🏆</span>
          <div>
            解锁新成就 ×{evaluation.new_achievements.length}
            <div style={{ fontSize: 11, fontWeight: 400, opacity: 0.9, marginTop: 2 }}>
              去「我的」查看详情
            </div>
          </div>
        </div>
      )}

      <div
        style={{
          background: 'linear-gradient(135deg, var(--primary-soft), #fff)',
          borderRadius: 14,
          padding: 16,
          marginBottom: 14,
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div style={{ fontSize: 13, color: 'var(--fg-muted)' }}>
            🐾 推测：<strong>{evaluation.animal_guess}</strong>
            {evaluation.matched_venue_name && <> · {evaluation.matched_venue_name}</>}
          </div>
          <div
            style={{
              background: 'var(--primary)',
              color: '#fff',
              borderRadius: 8,
              padding: '4px 10px',
              fontSize: 13,
              fontWeight: 700,
            }}
          >
            {evaluation.vibe_score}分
          </div>
        </div>
        <div
          style={{
            marginTop: 10,
            display: 'inline-block',
            background: 'var(--accent)',
            color: 'white',
            borderRadius: 8,
            padding: '6px 12px',
            fontSize: 14,
            fontWeight: 700,
          }}
        >
          🏅 {evaluation.badge}
        </div>
        <div style={{ marginTop: 12, fontSize: 13, color: 'var(--fg-muted)', lineHeight: 1.6 }}>
          「{evaluation.caption}」
        </div>
      </div>

      <div
        style={{
          background: 'var(--bg-elev)',
          border: '1px solid var(--border)',
          borderRadius: 14,
          padding: 14,
          marginBottom: 14,
          lineHeight: 1.6,
          fontSize: 14,
          color: 'var(--fg)',
        }}
      >
        {evaluation.comment}
      </div>

      {evaluation.tips && evaluation.tips.length > 0 && (
        <div style={{ marginBottom: 14 }}>
          <div
            style={{
              fontSize: 12,
              color: 'var(--fg-muted)',
              marginBottom: 4,
            }}
          >
            📷 拍摄小贴士
          </div>
          {evaluation.tips.map((t: string, i: number) => (
            <div
              key={i}
              style={{
                fontSize: 13,
                color: 'var(--primary-strong)',
                background: 'var(--primary-soft)',
                padding: '6px 10px',
                borderRadius: 8,
                marginBottom: 4,
              }}
            >
              · {t}
            </div>
          ))}
        </div>
      )}

      {evaluation.fallback && (
        <div
          style={{
            fontSize: 11,
            color: 'var(--fg-muted)',
            background: '#fef3c7',
            padding: 8,
            borderRadius: 8,
            marginBottom: 14,
          }}
        >
          ⓘ LLM 不可用，使用规则引擎兜底评价
        </div>
      )}

      <div className="modal-actions">
        <button className="btn btn-ghost btn-full" onClick={onAgain}>
          🔄 再拍一张
        </button>
        <button className="btn btn-primary btn-full" onClick={onClose}>
          ✓ 完成
        </button>
      </div>
    </div>
  )
}