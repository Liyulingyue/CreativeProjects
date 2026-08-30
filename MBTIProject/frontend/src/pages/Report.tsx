import { useEffect, useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { api, type Report } from '../api'

export default function ReportPage() {
  const { sessionId } = useParams()
  const [report, setReport] = useState<Report | null>(null)
  const [error, setError] = useState('')

  useEffect(() => {
    if (!sessionId) return
    api.get<Report>(`/sessions/${sessionId}/report`).then(setReport).catch((e) => setError(String(e.message)))
  }, [sessionId])

  if (error) return <div className="error-banner">{error}</div>
  if (!report) return <div className="loading">正在生成报告…</div>

  return (
    <div className="report-page">
      <div className="report-hero">
        <p className="report-label">{report.assessment_name} · 测评报告</p>
        {report.type_code && <h1 className="type-code">{report.type_code}</h1>}
        {report.type_name && <p className="type-name">{report.type_name}</p>}
        {report.keywords.length > 0 && (
          <div className="keywords">
            {report.keywords.map((k) => (
              <span key={k} className="tag">{k}</span>
            ))}
          </div>
        )}
      </div>

      <section className="panel">
        <h3>维度画像</h3>
        {report.dimension_scores.map((d) => (
          <div key={d.key} className="dim-row">
            <div className="dim-head">
              <span>{d.name}</span>
              <span className="dim-pole">{d.pole} · {d.percent} 分</span>
            </div>
            <div className="progress-track">
              <div className="progress-fill" style={{ width: `${d.percent}%` }} />
            </div>
          </div>
        ))}
      </section>

      <section className="panel">
        <h3>
          深度解读
          <span className={`badge ${report.narrative_source === 'llm' ? 'badge-llm' : 'badge-template'}`}>
            {report.narrative_source === 'llm' ? '✨ AI 深度解读' : '模板解读'}
          </span>
        </h3>
        <p className="narrative">{report.narrative}</p>
      </section>

      <section className="panel">
        <h3>建议</h3>
        <p className="narrative">{report.advice}</p>
      </section>

      <div className="report-actions">
        <Link className="btn btn-primary" to="/">回到测评广场</Link>
      </div>
    </div>
  )
}
