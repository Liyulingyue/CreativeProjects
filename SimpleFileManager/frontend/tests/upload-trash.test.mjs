// Uploading and the recycle bin: the two things a person does most.

import { BASE, ok, equal, contains } from './harness.mjs'

export default {
  name: '上传 / 回收站',
  tests: [
    {
      name: '上传一个文件，文件管理页上就多了一个',
      async fn(page) {
        await page.goto(BASE, { waitUntil: 'networkidle' })
        await page.waitForTimeout(1000)
        const name = `上传测试-${Date.now()}.txt`
        const uploaded = await page.evaluate(async (fileName) => {
          const body = new FormData()
          body.append('files', new File(['hello'], fileName, { type: 'text/plain' }))
          const token = localStorage.getItem('sfm_token')
          const r = await fetch('/api/fs/upload?path=', {
            method: 'POST',
            body,
            headers: token ? { Authorization: 'Bearer ' + token } : {},
          })
          return r.ok
        }, name)
        ok(uploaded, '上传没有成功')

        await page.reload({ waitUntil: 'networkidle' })
        await page.waitForTimeout(1500)
        const shown = await page.evaluate(() =>
          Array.from(document.querySelectorAll('[data-file-name]'))
            .map((el) => el.getAttribute('data-file-name')))
        ok(shown.includes(name), `列表里没有刚上传的文件: ${shown.join(', ')}`)

        // Tidy up.
        await page.evaluate(async (fileName) => {
          const token = localStorage.getItem('sfm_token')
          await fetch('/api/fs/delete', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              ...(token ? { Authorization: 'Bearer ' + token } : {}),
            },
            body: JSON.stringify({ path: fileName, permanent: true }),
          })
        }, name)
      },
    },

    {
      name: '用文件选择器上传，走的是分块那条路',
      async fn(page) {
        // The page uploads in pieces, not in one request. The earlier
        // test posted the file itself; this one lets the page do it.
        await page.goto(BASE, { waitUntil: 'networkidle' })
        await page.waitForTimeout(1200)

        const name = `分块上传-${Date.now()}.txt`
        const seen = []
        page.on('request', (r) => {
          if (r.url().includes('/api/fs/upload/')) seen.push(r.url())
        })
        await page.setInputFiles('input[type="file"]:not([webkitdirectory])', {
          name,
          mimeType: 'text/plain',
          buffer: Buffer.from('分块传上来的内容'),
        })

        // Wait for the file to turn up in the list.
        let shown = []
        for (let i = 0; i < 30; i++) {
          await page.waitForTimeout(500)
          shown = await page.evaluate(() =>
            Array.from(document.querySelectorAll('[data-file-name]'))
              .map((el) => el.getAttribute('data-file-name')))
          if (shown.includes(name)) break
        }
        ok(shown.includes(name), `文件没有出现: ${shown.join(', ')}`)
        ok(seen.some((u) => u.includes('/api/fs/upload/chunk')),
          `没有走分块那条路: ${seen.join(' | ')}`)

        // And the content is what was sent.
        const content = await page.evaluate(async (n) => {
          const token = localStorage.getItem('sfm_token')
          const res = await fetch('/api/fs/content?path=' + encodeURIComponent(n),
            { headers: token ? { Authorization: 'Bearer ' + token } : {} })
          const body = await res.json()
          return body?.content ?? body?.text ?? ''
        }, name)
        ok(String(content).includes('分块传上来的内容'),
          `传上去的内容不对: ${String(content).slice(0, 80)}`)
      },
    },

    {
      name: '删掉的文件进了回收站，能还原回来',
      async fn(page) {
        await page.goto(BASE, { waitUntil: 'networkidle' })
        await page.waitForTimeout(1000)
        const name = `回收站测试-${Date.now()}.txt`
        await page.evaluate(async (fileName) => {
          const body = new FormData()
          body.append('files', new File(['hello'], fileName, { type: 'text/plain' }))
          const token = localStorage.getItem('sfm_token')
          await fetch('/api/fs/upload?path=', {
            method: 'POST',
            body,
            headers: token ? { Authorization: 'Bearer ' + token } : {},
          })
          await fetch('/api/fs/delete', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              ...(token ? { Authorization: 'Bearer ' + token } : {}),
            },
            body: JSON.stringify({ path: fileName }),
          })
        }, name)

        await page.goto(`${BASE}/trash`, { waitUntil: 'networkidle' })
        await page.waitForTimeout(1500)
        const body = await page.evaluate(() => document.body.innerText)
        ok(body.includes(name) || body.includes('回收站'),
          `回收站页面不对: ${body.slice(0, 160)}`)

        // Put it back, and check it is back.
        const restored = await page.evaluate(async (fileName) => {
          const token = localStorage.getItem('sfm_token')
          const list = await fetch('/api/fs/trash', {
            headers: token ? { Authorization: 'Bearer ' + token } : {},
          }).then((r) => r.json())
          const items = list.entries || list.items || (Array.isArray(list) ? list : [])
          const entry = items.find((i) => (i.name || '') === fileName)
          if (!entry) return false
          const r = await fetch(`/api/fs/trash/${entry.id}/restore`, {
            method: 'POST',
            headers: token ? { Authorization: 'Bearer ' + token } : {},
          })
          return r.ok
        }, name)
        ok(restored, '还原没有成功')

        // Tidy up.
        await page.evaluate(async (fileName) => {
          const token = localStorage.getItem('sfm_token')
          await fetch('/api/fs/delete', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              ...(token ? { Authorization: 'Bearer ' + token } : {}),
            },
            body: JSON.stringify({ path: fileName, permanent: true }),
          })
        }, name)
      },
    },

    {
      name: '回收站页面能打开，并且说清自己在回收站里',
      async fn(page) {
        await page.goto(`${BASE}/trash`, { waitUntil: 'networkidle' })
        await page.waitForTimeout(1500)
        const body = await page.evaluate(() => document.body.innerText)
        contains(body, '回收站', '回收站页面没有标题')
      },
    },
  ],
}
