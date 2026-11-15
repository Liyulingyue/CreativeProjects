// What the page says when the server has a problem.
//
// The backend answers a failure in Chinese, and the page used to throw
// that away and show "Failed to delete" instead. These tests watch the
// messages the page really shows.

import { BASE, ok, equal, contains } from './harness.mjs'

export default {
  name: '出错了说什么',
  tests: [
    {
      name: '删除失败时，页面上是服务器说的话，不是英文',
      async fn(page) {
        await page.goto(BASE, { waitUntil: 'networkidle' })
        await page.waitForTimeout(1000)
        await page.route('**/api/fs/delete', (route) =>
          route.fulfill({
            status: 400,
            contentType: 'application/json',
            body: JSON.stringify({ detail: '这个文件正在被使用，删不掉' }),
          }))
        // Trigger a delete through the page's own code path.
        const message = await page.evaluate(async () => {
          const mod = await import('/src/api.ts')
          try {
            await mod.deletePath('anything.txt')
            return ''
          } catch (e) {
            return String(e.message || e)
          }
        })
        ok(message.includes('正在被使用'), `没有显示服务器的话: ${message}`)
        ok(!/Failed/i.test(message), `显示了英文: ${message}`)
        await page.unroute('**/api/fs/delete')
      },
    },

    {
      name: '服务器什么也没说时，兜底的话也是中文',
      async fn(page) {
        await page.goto(BASE, { waitUntil: 'networkidle' })
        await page.waitForTimeout(1000)
        await page.route('**/api/fs/delete', (route) =>
          route.fulfill({ status: 500, contentType: 'text/plain', body: 'oops' }))
        const message = await page.evaluate(async () => {
          const mod = await import('/src/api.ts')
          try {
            await mod.deletePath('anything.txt')
            return ''
          } catch (e) {
            return String(e.message || e)
          }
        })
        ok(message.length > 0, '什么话都没有')
        ok(!/Failed/i.test(message), `显示了英文: ${message}`)
        ok(/[\u4e00-\u9fff]/.test(message), `兜底的话不是中文: ${message}`)
        await page.unroute('**/api/fs/delete')
      },
    },

    {
      name: '服务器返回 500 时，页面上有一句中文，而不是空白',
      async fn(page) {
        await page.goto(BASE, { waitUntil: 'networkidle' })
        await page.waitForTimeout(1000)
        await page.route('**/api/fs/tree*', (route) =>
          route.fulfill({
            status: 500,
            contentType: 'application/json',
            body: JSON.stringify({ detail: '服务器出了点问题，请稍后再试。' }),
          }))
        await page.reload({ waitUntil: 'networkidle' })
        await page.waitForTimeout(2000)

        const body = await page.evaluate(() => document.body.innerText)
        ok(body.length > 0, '页面空了')
        ok(!body.includes('Failed to'), `页面上有英文: ${body.slice(0, 200)}`)
        await page.unroute('**/api/fs/tree*')
      },
    },
  ],
}
