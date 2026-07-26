import { useEffect, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { api } from '../api/client'
import { PhotoWallFlow } from '../components/flows/PhotoWallFlow'
import { loadPhotoLog, appendPhotoLog, generateThumbnail, generatePreview } from '../lib/storage'

export function PhotoWallPage() {
  const navigate = useNavigate()
  const fileInputRef = useRef<HTMLInputElement>(null)
  const [file, setFile] = useState<File | null>(null)
  const [previewUrl, setPreviewUrl] = useState<string | null>(null)
  const [evaluation, setEvaluation] = useState<any | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [step, setStep] = useState<'idle' | 'preview' | 'evaluating' | 'result' | 'error'>('idle')

  function handleFileSelected(e: React.ChangeEvent<HTMLInputElement>) {
    const f = e.target.files?.[0]
    if (!f) return
    if (f.size > 8 * 1024 * 1024) {
      setError('图片太大（最大 8MB）')
      setStep('error')
      return
    }
    setFile(f)
    setPreviewUrl(URL.createObjectURL(f))
    setStep('preview')
    e.target.value = ''
  }

  async function submit() {
    if (!file) return
    setStep('evaluating')
    setError(null)
    try {
      const result = await api.evaluatePhoto(file, file.name)
      setEvaluation(result)
      try {
        const thumbnail = await generateThumbnail(file)
        const preview = await generatePreview(file)
        appendPhotoLog({
          evaluation_id: result.evaluation_id,
          animal: result.animal || '未知动物',
          desc: result.desc,
          score: result.score,
          style: result.style || '其他',
          blurry: result.blurry || '清晰',
          ts: result.ts,
          source: 'wall',
          thumbnail,
          preview,
        })
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
    setStep('idle')
  }

  return (
    <div className="fullscreen-flow">
      <input
        ref={fileInputRef}
        type="file"
        accept="image/*"
        style={{ display: 'none' }}
        onChange={handleFileSelected}
      />
      <PhotoWallFlow
        onClose={() => navigate('/activity')}
        onOpenPhoto={() => fileInputRef.current?.click()}
      />

      {step !== 'idle' && (
        <div className="flow-modal-overlay">
          <div className="fullscreen-flow">
            <header className="flow-header">
              <button className="flow-back" onClick={reset}>←</button>
              <div className="flow-title">
                {step === 'preview' && '📷 评价出片'}
                {step === 'evaluating' && '📷 评价中'}
                {step === 'result' && '📷 评价结果'}
                {step === 'error' && '📷 出错了'}
              </div>
              <div style={{ width: 36 }} />
            </header>

            <div className="flow-body">
              {step === 'preview' && previewUrl && (
                <>
                  <img
                    src={previewUrl}
                    alt="预览"
                    style={{ width: '100%', maxHeight: 360, objectFit: 'cover', borderRadius: 12, marginBottom: 12 }}
                  />
                  {error && <div className="error-banner" style={{ marginBottom: 12 }}>{error}</div>}
                  <div className="modal-actions">
                    <button className="btn btn-ghost btn-full" onClick={reset}>取消</button>
                    <button className="btn btn-primary btn-full" onClick={submit}>✨ 评分</button>
                  </div>
                </>
              )}

              {step === 'evaluating' && (
                <div className="loading">
                  <div className="spinner" />
                  正在评价出片…
                </div>
              )}

              {step === 'result' && evaluation && (
                 <>
                   <div
                     style={{
                       background: 'linear-gradient(135deg, var(--primary-soft), #fff)',
                       borderRadius: 14,
                       padding: 16,
                       marginBottom: 14,
                     }}
                   >
                     <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                       <div style={{ fontSize: 15, fontWeight: 600, color: 'var(--fg)' }}>
                         {evaluation.animal || '未知动物'}
                       </div>
                       <div style={{ background: evaluation.score >= 80 ? '#10b981' : evaluation.score >= 50 ? '#f59e0b' : 'var(--fg-muted)', color: '#fff', borderRadius: 8, padding: '4px 10px', fontSize: 13, fontWeight: 700 }}>
                         {evaluation.score}分
                       </div>
                     </div>
                     {evaluation.desc && (
                       <div style={{ marginTop: 10, fontSize: 13, color: 'var(--fg)', lineHeight: 1.6 }}>
                         {evaluation.desc}
                       </div>
                     )}
                     <div style={{ display: 'flex', gap: 6, marginTop: 10 }}>
                       {evaluation.style && evaluation.style !== '其他' && (
                         <span style={{ fontSize: 11, background: 'var(--primary-soft)', color: 'var(--primary-strong)', padding: '2px 8px', borderRadius: 4 }}>
                           {evaluation.style}
                         </span>
                       )}
                       {evaluation.blurry && (
                         <span style={{ fontSize: 11, background: evaluation.blurry === '清晰' ? '#d1fae5' : evaluation.blurry === '略微模糊' ? '#fef3c7' : '#fee2e2', color: 'var(--fg-muted)', padding: '2px 8px', borderRadius: 4 }}>
                           {evaluation.blurry}
                         </span>
                       )}
                     </div>
                   </div>
                   <div className="modal-actions">
                     <button className="btn btn-primary btn-full" onClick={reset}>✓ 完成</button>
                   </div>
                 </>
               )}

              {step === 'error' && (
                <>
                  <div className="error-banner">{error}</div>
                  <button className="btn btn-primary btn-full" onClick={reset}>重试</button>
                </>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
