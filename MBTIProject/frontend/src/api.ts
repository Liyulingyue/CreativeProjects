// 与后端 schemas.py 对应的类型定义
export interface Option { label: string; value: number }

export interface Dimension {
  key: string; name: string; pole_high: string; pole_low: string; description: string
}

export interface Question {
  id: string; text: string; dimension: string; reverse: boolean; options: Option[]
}

export interface ResultType {
  code: string; name: string; keywords: string[]; description: string; advice: string
}

export interface Assessment {
  id: string; name: string; description: string
  subject: 'human' | 'cat' | 'other'
  report_type: 'type_matching' | 'dimension_profile'
  time_estimate_minutes: number
  source: 'builtin' | 'ai_generated'
  dimensions: Dimension[]; questions: Question[]; types: ResultType[]
}

export interface Answer { question_id: string; value: number }

export interface SessionState {
  session_id: string; assessment_id: string
  status: 'ongoing' | 'finished'
  answers: Answer[]; current_index: number
}

export interface DimensionScore { key: string; name: string; raw: number; percent: number; pole: string }

export interface Report {
  session_id: string; assessment_id: string; assessment_name: string
  type_code: string | null; type_name: string | null; keywords: string[]
  type_description: string; advice: string
  dimension_scores: DimensionScore[]
  narrative: string; narrative_source: 'llm' | 'template'
}

export interface CheckItem { name: string; passed: boolean; detail: string }

export interface LlmReview {
  valid: number | null; clarity: number | null; scoring_logic: number | null; overall: number | null
  issues: string[]; suggestions: string[]; summary: string
}

export interface EvaluationReport {
  assessment_id: string; assessment_name: string
  structured: { question_count: number; dimension_count: number; reverse_ratio: number; coverage_ok: boolean; items: CheckItem[] }
  llm_review: LlmReview | null
  verdict: string
}

async function handle<T>(resp: Response): Promise<T> {
  if (!resp.ok) {
    const body = await resp.json().catch(() => ({ detail: resp.statusText }))
    throw new Error(body.detail ?? `请求失败（${resp.status}）`)
  }
  return resp.json()
}

export const api = {
  get: <T,>(path: string) => fetch(`/api${path}`).then((r) => handle<T>(r)),
  post: <T,>(path: string, body?: unknown) =>
    fetch(`/api${path}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    }).then((r) => handle<T>(r)),
}
