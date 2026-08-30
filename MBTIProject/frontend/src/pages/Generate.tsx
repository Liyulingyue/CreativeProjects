import { useState } from 'react'
import { Link } from 'react-router-dom'
import { api, type Assessment } from '../api'

export default function Generate() {
  const [topic, setTopic] = useState('')
  const [subject, setSubject] = useState('human')
  const [questionCount, setQuestionCount] = useState(12)
  const [dimensionCount, setDimensionCount] = useState(4)
  const [extra, setExtra] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [created, setCreated] = useState<Assessment | null>(null)

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!topic.trim() || busy) return
    setBusy(true)
    setError('')
    setCreated(null)
    try {
      const a = await api.post<Assessment>('/assessments/generate', {
        topic,
        subject,
        question_count: questionCount,
        dimension_count: dimensionCount,
        extra_requirements: extra,
      })
      setCreated(a)
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="generate-page">
      <section className="hero">
        <h1>✨ AI 生成测评</h1>
        <p>输入任何想测的主题——「领导力风格」「拖延倾向」「狗狗性格」——AI 会设计维度、题目、计分规则与结果模板，并自动入库。生成后还可以对它做一次「合理性评价」。</p>
      </section>

      <form className="panel form-grid" onSubmit={submit}>
        <label className="form-item span2">
          测评主题 *
          <input value={topic} onChange={(e) => setTopic(e.target.value)} placeholder="例如：职场领导力风格" required />
        </label>
        <label className="form-item">
          测评对象
          <select value={subject} onChange={(e) => setSubject(e.target.value)}>
            <option value="human">人类</option>
            <option value="cat">猫咪</option>
            <option value="other">其他</option>
          </select>
        </label>
        <label className="form-item">
          题目数量
          <input type="number" min={4} max={40} value={questionCount} onChange={(e) => setQuestionCount(+e.target.value)} />
        </label>
        <label className="form-item">
          维度数量
          <input type="number" min={2} max={8} value={dimensionCount} onChange={(e) => setDimensionCount(+e.target.value)} />
        </label>
        <label className="form-item span2">
          补充要求（可选）
          <textarea rows={2} value={extra} onChange={(e) => setExtra(e.target.value)} placeholder="例如：面向团队管理者，题目要贴近职场场景" />
        </label>
        <div className="span2">
          <button className="btn btn-primary" disabled={busy || !topic.trim()}>
            {busy ? '生成中，约需 30 秒…' : '生成测评'}
          </button>
        </div>
      </form>

      {error && <div className="error-banner">{error}</div>}

      {created && (
        <section className="panel">
          <h3>
            {created.name}
            <span className="tag tag-ai">✨ 已入库</span>
          </h3>
          <p className="card-desc">{created.description}</p>
          <p className="card-meta">
            {created.dimensions.length} 个维度：{created.dimensions.map((d) => d.name).join(' / ')} · {created.questions.length} 题
          </p>
          <ol className="preview-questions">
            {created.questions.slice(0, 5).map((q) => (
              <li key={q.id}>
                {q.text}
                <span className="tag">[{created.dimensions.find((d) => d.key === q.dimension)?.name ?? q.dimension}{q.reverse ? ' · 反向' : ''}]</span>
              </li>
            ))}
            {created.questions.length > 5 && <li>…共 {created.questions.length} 题</li>}
          </ol>
          <div className="card-actions">
            <Link className="btn btn-primary" to={`/take/${created.id}`}>去做这份测评</Link>
            <Link className="btn btn-ghost" to={`/evaluate/${created.id}`}>查看合理性评价</Link>
          </div>
        </section>
      )}
    </div>
  )
}
