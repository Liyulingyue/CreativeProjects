import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { api, type Assessment } from '../api'

const SUBJECT_LABEL: Record<Assessment['subject'], string> = {
  human: '👤 人类',
  cat: '🐱 猫咪',
  other: '🧩 其他',
}

export default function Home() {
  const [assessments, setAssessments] = useState<Assessment[]>([])
  const [error, setError] = useState('')

  useEffect(() => {
    api.get<Assessment[]>('/assessments').then(setAssessments).catch((e) => setError(String(e.message)))
  }, [])

  return (
    <div>
      <section className="hero">
        <h1>发现你的下一个测评</h1>
        <p>从猫格到 MBTI，从领导力到任何你能想到的主题——每份测评都由统一的维度与计分引擎驱动，也可以由 AI 现场生成。</p>
      </section>
      {error && <div className="error-banner">{error}</div>}
      <div className="card-grid">
        {assessments.map((a) => (
          <div key={a.id} className="card">
            <div className="card-tags">
              <span className="tag">{SUBJECT_LABEL[a.subject]}</span>
              {a.source === 'ai_generated' && <span className="tag tag-ai">✨ AI 生成</span>}
            </div>
            <h3>{a.name}</h3>
            <p className="card-desc">{a.description}</p>
            <div className="card-meta">
              {a.questions.length} 题 · 约 {a.time_estimate_minutes} 分钟 · {a.dimensions.length} 个维度
            </div>
            <div className="card-actions">
              <Link className="btn btn-primary" to={`/take/${a.id}`}>开始测评</Link>
              <Link className="btn btn-ghost" to={`/evaluate/${a.id}`}>合理性评价</Link>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
