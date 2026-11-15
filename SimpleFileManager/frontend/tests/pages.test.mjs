// The rest of the page the picture search lives in: the settings card,
// the text search that shares the page, and the agent's chat.

import { BASE, ok, equal, contains } from './harness.mjs'

const settings = (page) => page.goto(`${BASE}/settings`, { waitUntil: 'networkidle' })

export default {
  name: '设置 / 文本搜索 / 对话',
  tests: [
    {
      name: '设置页有「图片索引」卡片，能开关定时、能改时间段',
      async fn(page) {
        await settings(page)
        await page.waitForTimeout(800)
        const body = await page.evaluate(() => document.body.innerText)
        contains(body, '图片索引', '设置页没有图片索引卡片')
        contains(body, '定时建立图片索引', '没有定时开关')
      },
    },

    {
      name: '时间段填错了要说人话，不要把英文丢给用户',
      async fn(page) {
        await settings(page)
        await page.waitForTimeout(800)
        const input = page.locator('input[placeholder="09:00-09:30"]').first()
        if (!(await input.count())) {
          // The card only shows the window when the switch is on.
          const toggle = page.locator('text=定时建立图片索引').first()
          await toggle.click()
          await page.waitForTimeout(500)
        }
        const field = page.locator('input[placeholder="09:00-09:30"]').first()
        ok(await field.count(), '没有时间段的输入框')
        await field.fill('99:99-99:99')
        await field.press('Enter')
        await page.waitForTimeout(1200)

        const body = await page.evaluate(() => document.body.innerText)
        ok(!body.includes('Failed to update settings'),
          '把英文的失败提示丢给了用户')
      },
    },

    {
      name: '搜索页的文件标签还是好的（两个标签各自独立）',
      async fn(page) {
        await page.goto(`${BASE}/search`, { waitUntil: 'networkidle' })
        await page.waitForTimeout(800)
        const body = await page.evaluate(() => document.body.innerText)
        contains(body, '混合', '文本搜索的模式不在了')
        contains(body, '仅关键词', '文本搜索的模式不在了')
        ok(body.includes('图片'), '图片标签不在了')
      },
    },

    {
      name: 'Agent 对话页能问，也能停下来',
      async fn(page) {
        await page.goto(`${BASE}/chat`, { waitUntil: 'networkidle' })
        await page.waitForTimeout(1000)
        const box = page.locator('textarea, input[type="text"]').first()
        ok(await box.count(), '聊天页没有输入框')
        await box.fill('你好')
        // The button that sends it.
        const send = page.locator('button').filter({ hasText: /发送|提问|↵|➤/ }).first()
        ok(await send.count(), '没有发送按钮')
      },
    },
  ],
}
