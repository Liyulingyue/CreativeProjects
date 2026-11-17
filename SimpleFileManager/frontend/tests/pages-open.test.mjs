// Every page opens, renders, and does not call an API that fails.
//
// The pages that no test ever visited are the ones where a white
// screen hides until somebody clicks that entry in the sidebar. This
// walks the whole list and says so when one of them breaks.

import { BASE, ok } from './harness.mjs'

const PAGES = [
  { path: '/', name: '文件管理' },
  { path: '/search', name: '搜索' },
  { path: '/index', name: '索引' },
  { path: '/chat', name: '对话' },
  { path: '/agent-tools', name: 'Agent 能力' },
  { path: '/organizer', name: '整理' },
  { path: '/digest', name: '日报' },
  { path: '/trash', name: '回收站' },
  { path: '/duplicates', name: '重复文件' },
  { path: '/favorites', name: '收藏' },
  { path: '/shares', name: '共享链接' },
  { path: '/settings', name: '设置' },
]

export default {
  name: '每个页面都打得开',
  tests: [
    {
      name: '十二个页面都渲染出来，也不报错',
      async fn(page) {
        const problems = []

        for (const p of PAGES) {
          const errors = []
          const failed = []

          const onError = (e) => errors.push(String(e).slice(0, 120))
          const onResponse = (r) => {
            const url = r.url()
            // Only the API: a missing icon is not a broken page.
            if (url.includes('/api/') && r.status() >= 400 && r.status() !== 401) {
              failed.push(`${r.status()} ${url.replace(BASE, '')}`)
            }
          }
          page.on('pageerror', onError)
          page.on('response', onResponse)

          await page.goto(BASE + p.path, { waitUntil: 'networkidle' })
          await page.waitForTimeout(1200)

          // The sidebar is on every page, so its text says nothing
          // about whether the page itself rendered. The content area
          // is the div next to it.
          const content = await page.evaluate(() => {
            const aside = document.querySelector('aside')
            const main = aside && aside.nextElementSibling
            return {
              length: (main?.innerText || '').trim().length,
              text: (main?.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 80),
            }
          })
          if (content.length < 20) {
            problems.push(
              `${p.path}：内容区只有 ${content.length} 字，像是没渲染出来${content.text ? `（${content.text}）` : ''}`)
          }
          if (errors.length) {
            problems.push(`${p.path}：页面报错 ${errors[0]}`)
          }
          if (failed.length) {
            problems.push(`${p.path}：接口失败 ${failed.slice(0, 2).join(' | ')}`)
          }

          page.off('pageerror', onError)
          page.off('response', onResponse)
        }

        ok(problems.length === 0, '这些页面有问题：\n   ' + problems.join('\n   '))
      },
    },
  ],
}
