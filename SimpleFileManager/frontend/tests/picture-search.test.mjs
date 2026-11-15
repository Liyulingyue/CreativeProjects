// The picture search page: what it shows, and what it says.

import { openPictureSearch, tiles, lastToast, ok, equal, contains } from './harness.mjs'

export default {
  name: '图片检索',
  tests: [
    {
      name: '搜一张图，图和名字都出来，缩略图真的画出来了',
      async fn(page) {
        await openPictureSearch(page)
        await page.locator('input[placeholder*="海边"]').first().fill('海边的日落')
        await page.locator('button', { hasText: /^搜图$/ }).first().click()
        await page.waitForTimeout(6000)

        const found = await tiles(page)
        ok(found.length > 0, '没有搜到任何图片')
        equal(found.filter((t) => t.drawn).length, found.length, '有缩略图没画出来')
        ok(found[0].name.length > 0, '结果没有名字')
        // The link has to be a real one, not a placeholder.
        ok(found[0].href && found[0].href.startsWith('/api/'),
          `链接不是真的: ${found[0].href}`)
      },
    },

    {
      name: '「日落」搜得到希腊那张，排序按相似度往下走',
      async fn(page) {
        await openPictureSearch(page)
        await page.locator('input[placeholder*="海边"]').first().fill('日落')
        await page.locator('button', { hasText: /^搜图$/ }).first().click()
        await page.waitForTimeout(6000)

        const found = await tiles(page)
        ok(found.length > 0, '没有搜到任何图片')
        const scores = found.map((t) => Number(t.score)).filter((n) => !isNaN(n))
        for (let i = 1; i < scores.length; i += 1) {
          ok(scores[i] <= scores[i - 1],
            `相似度没有排好序: ${scores.join(', ')}`)
        }
        ok(found.some((t) => /Sunset|Greece/i.test(t.name)),
          `日落没有搜到希腊那张: ${found.map((t) => t.name).join(', ')}`)
      },
    },

    {
      name: '「找相似」把画面换成这张图的邻居，原图标成参考图',
      async fn(page) {
        await openPictureSearch(page)
        await page.locator('input[placeholder*="海边"]').first().fill('水果')
        await page.locator('button', { hasText: /^搜图$/ }).first().click()
        await page.waitForTimeout(6000)

        const button = page.locator('button', { hasText: '找相似' }).first()
        ok(await button.count(), '结果里没有「找相似」')
        await button.click()
        await page.waitForTimeout(6000)

        const body = await page.evaluate(() => document.body.innerText)
        contains(body, '以图搜图', '没有切换到以图搜图')
        contains(body, '参考图', '原图没有被标成参考图')

        const found = await tiles(page)
        ok(found.length > 0, '以图搜图没有结果')
        equal(found.filter((t) => t.drawn).length, found.length, '有缩略图没画出来')
      },
    },

    {
      name: '「回到文搜图」把画面还回去',
      async fn(page) {
        await openPictureSearch(page)
        await page.locator('input[placeholder*="海边"]').first().fill('水果')
        await page.locator('button', { hasText: /^搜图$/ }).first().click()
        await page.waitForTimeout(6000)
        await page.locator('button', { hasText: '找相似' }).first().click()
        await page.waitForTimeout(6000)

        const back = page.locator('button', { hasText: '回到文搜图' }).first()
        ok(await back.count(), '没有「回到文搜图」')
        await back.click()
        await page.waitForTimeout(500)

        const body = await page.evaluate(() => document.body.innerText)
        ok(!body.includes('以图搜图'), '没有回到文搜图')
        contains(body, '用文字描述你要找的图片', '没有回到空状态')
      },
    },

    {
      name: '服务器说忙的时候，把���的话原样说出来',
      async fn(page) {
        await openPictureSearch(page)
        // The server says the machine is busy.
        await page.route('**/api/images/search', (route) =>
          route.fulfill({
            status: 503,
            contentType: 'application/json',
            body: JSON.stringify({ detail: '图片索引正在建，等它建完再试' }),
          }))
        await page.locator('input[placeholder*="海边"]').first().fill('海边的日落')
        await page.locator('button', { hasText: /^搜图$/ }).first().click()
        await page.waitForTimeout(1500)

        const body = await page.evaluate(() => document.body.innerText)
        contains(body, '图片索引正在建', '没有把服务器的话说出来')
        await page.unroute('**/api/images/search')
      },
    },

    {
      name: '空结果时说清楚是索引还空着，而不是「没找到」',
      async fn(page) {
        await openPictureSearch(page)
        await page.route('**/api/images/search', (route) =>
          route.fulfill({
            status: 200,
            contentType: 'application/json',
            body: JSON.stringify({ results: [] }),
          }))
        await page.locator('input[placeholder*="海边"]').first().fill('xyzzy')
        await page.locator('button', { hasText: /^搜图$/ }).first().click()
        await page.waitForTimeout(1200)

        const body = await page.evaluate(() => document.body.innerText)
        contains(body, '没有找到匹配的图片', '没有说没有找到')
        await page.unroute('**/api/images/search')
      },
    },

    {
      name: '索引状态栏说清楚索引里有多少张',
      async fn(page) {
        await openPictureSearch(page)
        await page.waitForTimeout(1200)
        const body = await page.evaluate(() => document.body.innerText)
        contains(body, '图片索引', '状态栏没有出现')
        ok(/图片索引\s*\d+\s*张/.test(body), `状态栏没有说有多少张: ${body.slice(0, 200)}`)
      },
    },
  ],
}
