import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { api, type Assessment, type SessionState } from '../api'

export default function Take() {
  const { assessmentId } = useParams()
  const navigate = useNavigate()
  const [assessment, setAssessment] = useState<Assessment | null>(null)
  const [session, setSession] = useState<SessionState | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (!assessmentId) return
    Promise.all([
      api.get<Assessment>(`/assessments/${assessmentId}`),
      api.post<SessionState>(`/assessments/${assessmentId}/sessions`),
    ])
      .then(([a, s]) => {
        setAssessment(a)
        setSession(s)
      })
      .catch((e) => setError(String(e.message)))
  }, [assessmentId])

  const answer = async (value: number) => {
    if (!assessment || !session || busy) return
    const q = assessment.questions[session.current_index]
    if (!q) return
    setBusy(true)
    try {
      const updated = await api.post<SessionState>(`/sessions/${session.session_id}/answers`, {
        question_id: q.id,
        value,
      })
      if (updated.status === 'finished') {
        navigate(`/report/${updated.session_id}`)
      } else {
        setSession(updated)
      }
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  if (error) return <div className="error-banner">{error}</div>
  if (!assessment || !session) return <div className="loading">加载中…</div>

  const q = assessment.questions[session.current_index]
  const progress = Math.round((session.current_index / assessment.questions.length) * 100)

  return (
    <div className="take-page">
      <div className="take-head">
        <h2>{assessment.name}</h2>
        <span className="take-count">
          {session.current_index + 1} / {assessment.questions.length}
        </span>
      </div>
      <div className="progress-track">
        <div className="progress-fill" style={{ width: `${progress}%` }} />
      </div>
      <div className="question-card">
        <p className="question-hint">
          请根据{assessment.subject === 'cat' ? '你家猫咪' : '你的真实情况'}作答，直觉第一印象往往最准。
        </p>
        <h3 className="question-text">{q?.text}</h3>
        <div className="options">
          {(q?.options ?? []).map((opt) => (
            <button key={opt.value} className="option-btn" disabled={busy} onClick={() => answer(opt.value)}>
              {opt.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}
