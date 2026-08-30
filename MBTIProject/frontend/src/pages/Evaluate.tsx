import { useEffect, useState } from 'react'
import { useParams } from 'react-router-dom'
import { api, type EvaluationReport } from '../api'

export default function Evaluate() {
  const { assessmentId } = useParams()
  const [report, setReport] = useState<EvaluationReport | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(true)

  useEffect(() => {
    if (!assessmentId) return
    setBusy(true)
    api
      .post<EvaluationReport>(`/assessments/${assessmentId}/evaluation`)
      .then(setReport)
      .catch((e) => setError(String(e.message)))
      .finally(() => setBusy(false))
  }, [assessmentId])

  if (busy) return <div className="loading">正在进行合理性评价…</div>
  if (error) return <div className="error-banner">{error}</div>
  if (!report) return null

  const review = report.llm_review
  const scores = review
    ? [
        { label: '内容效度', value: review.valid },
        { label: '表述清晰度', value: review.clarity },
        { label: '计分逻辑', value: review.scoring_logic },
        { label: '综合评分', value: review.overall },
      ]
    : []

  return (
    <div className="evaluate-page">
      <section className="hero">
        <h1>合理性评价 · {report.assessment_name}</h1>
        <p>包含两部分：不依赖大模型的结构化体检（题量、维度覆盖、反向题比例等），以及 LLM 作为评审专家的质性评价。</p>
      </section>

      <div className={`verdict ${report.verdict.includes('通过') ? 'verdict-ok' : 'verdict-warn'}`}>{report.verdict}</div>

      <section className="panel">
        <h3>结构化体检</h3>
        <div className="stat-row">
          <div className="stat"><b>{report.structured.question_count}</b><span>题目</span></div>
          <div className="stat"><b>{report.structured.dimension_count}</b><span>维度</span></div>
          <div className="stat"><b>{Math.round(report.structured.reverse_ratio * 100)}%</b><span>反向题占比</span></div>
          <div className="stat"><b>{report.structured.coverage_ok ? '✓' : '✗'}</b><span>维度全覆盖</span></div>
        </div>
        <ul className="check-list">
          {report.structured.items.map((item) => (
            <li key={item.name} className={item.passed ? 'check-ok' : 'check-bad'}>
              <b>{item.passed ? '✓' : '✗'} {item.name}</b>
              <span>{item.detail}</span>
            </li>
          ))}
        </ul>
      </section>

      {review ? (
        <section className="panel">
          <h3>LLM 质性评审</h3>
          <div className="stat-row">
            {scores.map((s) => (
              <div key={s.label} className="stat">
                <b>{s.value ?? '-'}</b>
                <span>{s.label}</span>
              </div>
            ))}
          </div>
          {review.summary && <p className="narrative">{review.summary}</p>}
          {review.issues.length > 0 && (
            <>
              <h4>发现的问题</h4>
              <ul>{review.issues.map((i) => <li key={i}>{i}</li>)}</ul>
            </>
          )}
          {review.suggestions.length > 0 && (
            <>
              <h4>改进建议</h4>
              <ul>{review.suggestions.map((s) => <li key={s}>{s}</li>)}</ul>
            </>
          )}
        </section>
      ) : (
        <section className="panel">
          <h3>LLM 质性评审</h3>
          <p className="card-desc">未配置 LLM_API_KEY，本次仅执行结构化体检。在 backend/.env 中配置后可自动补充质性评审。</p>
        </section>
      )}
    </div>
  )
}
