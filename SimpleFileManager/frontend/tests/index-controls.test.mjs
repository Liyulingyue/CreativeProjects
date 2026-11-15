// The index controls: the buttons, and what they say when they do it.

import { openPictureSearch, lastToast, ok, equal, contains } from './harness.mjs'

const button = (page, text) => page.locator('button', { hasText: text }).first()

export default {
  name: '图片索引的按钮',
  tests: [
    {
      name: '建索引的时候有「暂停」，跑完了又有「建立图片索引」',
      async fn(page) {
        await openPictureSearch(page)
        // The build is under way.
        await page.route('**/api/images/index/status', async (route) => {
          const response = await route.fetch()
          const body = await response.json()
          await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ ...body, running: true, total: 10, done: 3 }),
          })
        })
        // The status is polled every few seconds, so the page has to be
        // loaded again for the new answer to be the one on the screen.
        await page.reload({ waitUntil: 'networkidle' })
        await page.locator('button', { hasText: /^图片$/ }).first().click()
        await page.waitForTimeout(1200)
        ok(await button(page, '暂停').count(), '没有出现「暂停」')
        ok(!(await button(page, '建立图片索引').count()), '运行中还有「建立图片索引」')
        await page.unroute('**/api/images/index/status')
      },
    },

    {
      name: '暂停之后有「继续」',
      async fn(page) {
        await openPictureSearch(page)
        await page.route('**/api/images/index/status', async (route) => {
          const response = await route.fetch()
          const body = await response.json()
          await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ ...body, running: true, paused: true }),
          })
        })
        await page.reload({ waitUntil: 'networkidle' })
        await page.locator('button', { hasText: /^图片$/ }).first().click()
        await page.waitForTimeout(1200)
        ok(await button(page, '继续').count(), '没有出现「继续」')
        await page.unroute('**/api/images/index/status')
      },
    },

    {
      name: '服务器说「已经在跑了」，就不该说「已开始」',
      async fn(page) {
        await openPictureSearch(page)
        await page.route('**/api/images/index/start', (route) =>
          route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ running: true, started: false, reason: '图片索引已经在跑了' }),
          }))
        await page.reload({ waitUntil: 'networkidle' })
        await page.locator('button', { hasText: /^图片$/ }).first().click()
        await page.waitForTimeout(800)
        await button(page, '建立图片索引').click()
        await page.waitForTimeout(800)

        const toast = await lastToast(page)
        contains(toast, '已经在跑', `提示不对: ${toast}`)
        ok(!toast.includes('已开始'), `说了「已开始」: ${toast}`)
        await page.unroute('**/api/images/index/start')
      },
    },

    {
      name: '管理员才能建索引：普通用户看不到那些按钮',
      async fn(page) {
        await openPictureSearch(page)
        // The server says this is an ordinary user.
        await page.route('**/api/users/me', (route) =>
          route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ username: 'someone', is_admin: false, authenticated: true }),
          }))
        await page.reload({ waitUntil: 'networkidle' })
        await page.locator('button', { hasText: /^图片$/ }).first().click()
        await page.waitForTimeout(1200)

        ok(!(await button(page, '建立图片索引').count()), '普通用户看到了「建立图片索引」')
        await page.unroute('**/api/users/me')
      },
    },

    {
      name: '普通用户看到的是「请管理员建立图片索引」',
      async fn(page) {
        await openPictureSearch(page)
        await page.route('**/api/users/me', (route) =>
          route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ username: 'someone', is_admin: false, authenticated: true }),
          }))
        await page.route('**/api/images/search', (route) =>
          route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ results: [] }),
          }))
        // The index has to look empty, or the page says "try other
        // words" instead of "ask the administrator".
        await page.route('**/api/images/index/status', async (route) => {
          const response = await route.fetch()
          const body = await response.json()
          await route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ ...body, indexed: 0 }),
          })
        })
        await page.reload({ waitUntil: 'networkidle' })
        await page.locator('button', { hasText: /^图片$/ }).first().click()
        await page.waitForTimeout(800)
        await page.locator('input[placeholder*="海边"]').first().fill('xyzzy')
        await page.locator('button', { hasText: /^搜图$/ }).first().click()
        await page.waitForTimeout(1200)

        const body = await page.evaluate(() => document.body.innerText)
        contains(body, '请管理员', '没有告诉用户去找管理员')
        ok(!body.includes('建立图片索引」先把图片收进来'), '给普通用户说了管理员的话')
        await page.unroute('**/api/users/me')
        await page.unroute('**/api/images/search')
      },
    },
  ],
}
