// The messages the user reads have to be in Chinese.
//
// The page is in Chinese and an error that arrives in English is a
// sentence the user cannot read. The fallback of every request helper
// is such a message, so those are checked here.

import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { ok } from './harness.mjs'

const SRC = new URL('../src/', import.meta.url).pathname

function filesUnder(dir) {
  const out = []
  for (const name of readdirSync(dir)) {
    const full = join(dir, name)
    if (statSync(full).isDirectory()) out.push(...filesUnder(full))
    else if (/\.(ts|tsx)$/.test(name)) out.push(full)
  }
  return out
}

// apiError(res, '...'), planError(res, '...'), new Error('...')
const PATTERN = /(?:apiError|planError)\(\s*res\s*,\s*'([^']*)'\)|new Error\('([^']*)'\)/g

export default {
  name: '报错是中文的',
  tests: [
    {
      name: '前端给用户看的兜底消息都是中文',
      async fn(page) {
        const offenders = []
        for (const file of filesUnder(SRC)) {
          const text = readFileSync(file, 'utf-8')
          let m
          PATTERN.lastIndex = 0
          while ((m = PATTERN.exec(text)) !== null) {
            const message = m[1] ?? m[2] ?? ''
            // A message with no Chinese in it is a message the user
            // cannot read.
            if (message && !/[\u4e00-\u9fff]/.test(message)) {
              const line = text.slice(0, m.index).split('\n').length
              offenders.push(`${relative(SRC, file)}:${line}  ${message}`)
            }
          }
        }
        ok(offenders.length === 0,
          `这些消息还是英文：\n${offenders.join('\n')}`)
      },
    },
  ],
}
