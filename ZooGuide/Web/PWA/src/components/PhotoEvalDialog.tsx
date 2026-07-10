import { useEffect, useRef, useState } from 'react'
import type { NearestResponse, PhotoEvaluation } from '../types'
import { api } from '../api/client'

interface Props {
  onClose: () => void
  onPickVenue?: (venueId: string) => void
}

type Phase = 'select' | 'preview' | 'evaluating' | 'result'

export function PhotoEvalDialog({ onClose, onPickVenue }: Props) {
  const [phase, setPhase] = useState<Phase>('select')
  const [file, setFile] = useState<File | null>(null)
  const [previewUrl, setPreviewUrl] = useState<string | null>(null)
  const [evaluation, setEvaluation] = useState<PhotoEvaluation | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [usingCamera, setUsingCamera] = useState(false)
  const videoRef = useRef<HTMLVideoElement>(null)
  const streamRef = useRef<MediaStream | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    return () => {
      if (previewUrl) URL.revokeObjectURL(previewUrl)
      if (streamRef.current) {
        streamRef.current.getTracks().forEach((t) => t.stop())
      }
    }
  }, [previewUrl])

  function pickFile(f: File) {
    if (f.size > 8 * 1024 * 1024) {
      setError('图片太大（最大 8MB）')
      return
    }
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
      // Wait a tick for video element to render
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
      setPhase('result')
    } catch (e) {
      setError(e instanceof Error ? e.message : '评价失败')
      setPhase('preview')
    }
  }

  function reset() {
    if (previewUrl) URL.revokeObjectURL(previewUrl)
    setFile(null)
    setPreviewUrl(null)
    setEvaluation(null)
    setError(null)
    setPhase('select')
  }

  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 460 }}>
        <h3>📸 合照彩蛋</h3>

        {phase === 'select' && !usingCamera && (
          <>
            <p style={{ fontSize: 13, color: 'var(--fg-muted)', margin: '0 0 14px' }}>
              上传或拍一张在红山拍的动物/合照，Agent 会给你一段幽默点评 + 出片徽章。
            </p>
            <div style={{ display: 'flex', gap: 10 }}>
              <button
                className="btn btn-primary btn-full"
                onClick={() => fileInputRef.current?.click()}
              >
                📁 从相册选
              </button>
              <button className="btn btn-ghost btn-full" onClick={startCamera}>
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
          </>
        )}

        {phase === 'select' && usingCamera && (
          <>
            <video
              ref={videoRef}
              style={{ width: '100%', borderRadius: 12, background: '#000' }}
              playsInline
              muted
            />
            <div className="modal-actions">
              <button className="btn btn-ghost" onClick={stopCamera}>
                取消
              </button>
              <button className="btn btn-primary" onClick={captureFromCamera}>
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
                maxHeight: 280,
                objectFit: 'cover',
                borderRadius: 12,
                marginBottom: 12,
              }}
            />
            {error && <div className="error-banner">{error}</div>}
            <div className="modal-actions">
              <button className="btn btn-ghost" onClick={reset}>
                重选
              </button>
              <button className="btn btn-primary" onClick={submit}>
                ✨ 出片点评
              </button>
            </div>
          </>
        )}

        {phase === 'evaluating' && (
          <div className="loading">
            <div className="spinner" />
            正在给照片打分…
          </div>
        )}

        {phase === 'result' && evaluation && (
          <EvaluationResult
            evaluation={evaluation}
            onClose={onClose}
            onPickVenue={onPickVenue}
            onAgain={reset}
          />
        )}
      </div>
    </div>
  )
}

function EvaluationResult({
  evaluation,
  onClose,
  onPickVenue,
  onAgain,
}: {
  evaluation: PhotoEvaluation
  onClose: () => void
  onPickVenue?: (venueId: string) => void
  onAgain: () => void
}) {
  return (
    <>
      <div
        style={{
          background: 'linear-gradient(135deg, var(--primary-soft), #fff)',
          borderRadius: 12,
          padding: 14,
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
            color: '#fff',
            borderRadius: 8,
            padding: '6px 12px',
            fontSize: 14,
            fontWeight: 700,
          }}
        >
          🏅 {evaluation.badge}
        </div>
        <div style={{ marginTop: 12, fontSize: 13, color: 'var(--fg-muted)' }}>
          「{evaluation.caption}」
        </div>
      </div>

      <div
        style={{
          background: '#fff',
          border: '1px solid var(--border)',
          borderRadius: 12,
          padding: 14,
          marginBottom: 14,
          lineHeight: 1.6,
          fontSize: 14,
          color: '#1a3a2a',
        }}
      >
        {evaluation.comment}
      </div>

      {evaluation.tips && evaluation.tips.length > 0 && (
        <div style={{ marginBottom: 14 }}>
          <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginBottom: 4 }}>📷 拍摄小建议</div>
          {evaluation.tips.map((t, i) => (
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
          ⓘ 当前模型不支持图片识别，使用规则引擎兜底评价（同样有梗）
        </div>
      )}

      <div className="modal-actions">
        {evaluation.matched_venue_id && onPickVenue && (
          <button
            className="btn btn-ghost"
            onClick={() => {
              onPickVenue(evaluation.matched_venue_id)
              onClose()
            }}
          >
            📍 我在{evaluation.matched_venue_name}
          </button>
        )}
        <button className="btn btn-ghost" onClick={onAgain}>
          🔄 再拍一张
        </button>
        <button className="btn btn-primary" onClick={onClose}>
          完成
        </button>
      </div>
    </>
  )
}