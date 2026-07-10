import { useEffect, useState } from 'react'
import { api } from '../api/client'
import type { QuizOptions } from '../types'

interface Props {
  onLoaded: (opts: QuizOptions) => void
}

export function useQuizOptions(): QuizOptions | null {
  const [opts, setOpts] = useState<QuizOptions | null>(null)
  useEffect(() => {
    api.quizOptions().then(setOpts).catch(console.error)
  }, [])
  return opts
}