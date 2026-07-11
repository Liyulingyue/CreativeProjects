import { useEffect, useRef, useState } from 'react'
import type { Venue } from '../../types'
import { api } from '../../api/client'
import { loadPhotoLog, type PhotoLogEntry } from '../../lib/storage'

interface Props {
  venues: Venue[]
  onClose: () => void
}

type Step = 'select' | 'capture' | 'preview' | 'evaluating' | 'result' | 'error'

export function PhotoFlow({ venues, onClose }: Props) {
  const [step, setStep] = useState<Step>('select')
  const [venue, setVenue] = useState<Venue | null>(null)
  const [file, setFile] = useState<File | null>(null)
  const [previewUrl, setPreviewUrl] = useState<string | null>(null)
  const [evaluation, setEvaluation] = useState<any | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [usingCamera, setUsingCamera] = useState(false)
  const videoRef = useRef<HTMLVideoElement>(null)
  const streamRef = useRef<MediaStream | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const [history, setHistory] = useState<PhotoLogEntry[]>(loadPhotoLog())

  // Group venues by area
  const byArea: Record<string, Venue[]> = {}
  venues.forEach((v) => {
    const a = v.area || '其他'
    if (!byArea[a]) byArea[a] = []
    byArea[a].push(v)
  })

  function pickFile(f: File) {
    if (f.size > 8 * 1024 * 1024) {
      setError('图片太大（最大 8MB）')
      return
    }
    if (previewUrl) URL.revokeObjectURL(previewUrl)
    setFile(f)
    setPreviewUrl(URL.createObjectURL(f))
    setStep('preview')
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
    if (!file || !venue) return
    setStep('evaluating')
    setError(null)
    try {
      const result = await api.evaluatePhoto(file, file.name, {
        expectedVenueId: venue.id,
      })
      setEvaluation(result)
      // Append to local photo log
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
      setStep('result')
    } catch (e) {
      setError(e instanceof Error ? e.message : '评价失败')
      setStep('error')
    }
  }

  function reset() {
    if (previewUrl) URL.revokeObjectURL(previewUrl)
    setFile(null)
    setPreviewUrl(null)
    setEvaluation(null)
    setError(null)
    setStep('select')
    setVenue(null)
  }

  function selectVenue(v: Venue) {
    setVenue(v)
    setStep('capture')
  }

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button
          className="flow-back"
          onClick={() => {
            if (step === 'select') onClose()
            else reset()
          }}
        >
          ←
        </button>
        <div className="flow-title">
          {step === 'select' && '📷 选场馆'}
          {step === 'capture' && `📷 拍 ${venue?.name?.slice(0, 6)}…`}
          {step === 'preview' && '📷 预览'}
          {(step === 'evaluating' || step === 'result') && `📷 ${venue?.name?.slice(0, 6)}…`}
          {step === 'error' && '📷 出错了'}
        </div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        {/* Step 1: Select venue */}
        {step === 'select' && (
          <>
            <div
              style={{
                background: 'linear-gradient(135deg, #f59e0b, #d97706)',
                color: 'white',
                borderRadius: 14,
                padding: 18,
                textAlign: 'center',
                marginBottom: 14,
              }}
            >
              <div style={{ fontSize: 40, marginBottom: 6 }}>📷</div>
              <div style={{ fontSize: 18, fontWeight: 700 }}>先选场馆</div>
              <div style={{ fontSize: 12, opacity: 0.9, marginTop: 4 }}>
                选好后拍照，AI 会验证是否对应
              </div>
            </div>

            <div className="venue-selector">
              {Object.entries(byArea).map(([area, list]) => (
                <div key={area} className="venue-selector-section">
                  <div className="venue-selector-header">
                    <span>📍</span>
                    <span>{area}</span>
                    <span className="venue-selector-count">{list.length}</span>
                  </div>
                  <div className="venue-selector-grid">
                    {list.map((v) => (
                      <button
                        key={v.id}
                        className="venue-selector-tile"
                        onClick={() => selectVenue(v)}
                      >
                        <div className="vst-name">{v.name}</div>
                        <div className="vst-animals">{v.animals.slice(0, 2).join('·')}</div>
                      </button>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </>
        )}

        {/* Step 2: Capture */}
        {step === 'capture' && venue && !usingCamera && (
          <div
            style={{
              background: 'linear-gradient(135deg, var(--primary-soft), #fff)',
              borderRadius: 14,
              padding: 16,
              marginBottom: 14,
              border: '2px solid var(--primary)',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
              <div style={{ fontSize: 36 }}>📍</div>
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 11, color: 'var(--fg-muted)' }}>已选场馆</div>
                <div style={{ fontSize: 18, fontWeight: 700, color: 'var(--primary-strong)' }}>
                  {venue.name}
                </div>
                <div style={{ fontSize: 11, color: 'var(--fg-muted)' }}>
                  常见动物：{venue.animals.slice(0, 3).join('、')}
                </div>
              </div>
            </div>
          </div>
        )}

        {step === 'capture' && !usingCamera && (
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

        {step === 'capture' && usingCamera && (
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

        {/* Step 3: Preview + confirm */}
        {step === 'preview' && previewUrl && (
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
            {venue && (
              <div
                style={{
                  background: 'var(--primary-soft)',
                  borderRadius: 10,
                  padding: 10,
                  fontSize: 12,
                  color: 'var(--primary-strong)',
                  marginBottom: 12,
                  textAlign: 'center',
                }}
              >
                将验证是否匹配：<strong>{venue.name}</strong>
              </div>
            )}
            {error && <div className="error-banner" style={{ marginBottom: 10 }}>{error}</div>}
            <div className="modal-actions">
              <button className="btn btn-ghost btn-full" onClick={reset}>
                重选
              </button>
              <button className="btn btn-primary btn-full" onClick={submit}>
                ✨ 提交验证
              </button>
            </div>
          </>
        )}

        {step === 'evaluating' && (
          <div className="loading">
            <div className="spinner" />
            正在让 Agent 验证照片…
            <div style={{ fontSize: 12, marginTop: 8 }}>
              识别动物 + 比对「{venue?.name}」
            </div>
          </div>
        )}

        {step === 'result' && evaluation && venue && (
          <ResultCard
            evaluation={evaluation}
            expectedVenue={venue}
            onAgain={reset}
            onClose={onClose}
          />
        )}

        {step === 'error' && (
          <>
            <div className="error-banner">
              <strong>评价失败：</strong>
              {error}
            </div>
            <button className="btn btn-primary btn-full" onClick={() => setStep('capture')}>
              重试
            </button>
          </>
        )}

        {step === 'select' && history.length > 0 && (
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
  expectedVenue,
  onAgain,
  onClose,
}: {
  evaluation: any
  expectedVenue: Venue
  onAgain: () => void
  onClose: () => void
}) {
  const success = evaluation.success === true
  const actual = evaluation.matched_venue_name || evaluation.animal_guess || '未识别'

  return (
    <div>
      {/* Success/failure banner */}
      <div
        style={{
          background: success
            ? 'linear-gradient(135deg, #10b981, #059669)'
            : 'linear-gradient(135deg, #f59e0b, #d97706)',
          color: 'white',
          padding: '14px 16px',
          borderRadius: 12,
          marginBottom: 14,
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <div style={{ fontSize: 32 }}>{success ? '✓' : '⚠️'}</div>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 16, fontWeight: 700 }}>
              {success ? '打卡成功！' : '验证未通过'}
            </div>
            <div style={{ fontSize: 12, opacity: 0.95, marginTop: 2 }}>
              {success
                ? `已为「${expectedVenue.name}」自动打卡`
                : evaluation.failure_reason || `照片里没有 ${expectedVenue.name}`}
            </div>
          </div>
        </div>
        {success && evaluation.auto_checkin && (
          <div
            style={{
              fontSize: 11,
              marginTop: 6,
              opacity: 0.9,
            }}
          >
            🕓 {new Date(evaluation.auto_checkin.ts).toLocaleString('zh-CN')}
          </div>
        )}
      </div>

      {/* Achievement unlocked banner */}
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

      {/* Photo + AI analysis (always shown) */}
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
            {evaluation.matched_venue_name && (
              <> · {evaluation.matched_venue_name}</>
            )}
          </div>
          <div
            style={{
              background: success ? '#10b981' : '#f59e0b',
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