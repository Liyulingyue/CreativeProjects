// What the index buttons actually say, and what they actually do.
//
// The status endpoint answers "started: false" when a build is already
// running. A panel that ignores that answers "已开始建立图片索引" every
// time -- which is a lie the user sees twice.
//
//   node scripts/audit-image-index.mjs [baseUrl]

import { chromium } from 'playwright'

const BASE = process.argv[2] ?? 'http://127.0.0.1:15173'
const PASSWORD = process.env.SFM_PASSWORD
if (!PASSWORD) {
  console.error('set SFM_PASSWORD to the password of the box you are auditing')
  process.exit(2)
}

const toastText = async (page) => {
  await page.waitForTimeout(900)
  return page.evaluate(() => {
    const nodes = Array.from(document.querySelectorAll('[role="status"], [role="alert"], .toast, [class*="toast"]'))
    return nodes.map((n) => n.textContent.trim()).filter(Boolean).slice(-4)
  })
}

const clickIfThere = async (page, label) => {
  const b = page.locator('button', { hasText: label }).first()
  if (!(await b.count())) return false
  if (await b.isDisabled().catch(() => true)) return false
  await b.click().catch(() => {})
  return true
}

const run = async () => {
  const browser = await chromium.launch()
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 } })
  const page = await ctx.newPage()
  const errors = []
  page.on('pageerror', (e) => errors.push(String(e).slice(0, 160)))

  await page.goto(BASE, { waitUntil: 'networkidle' })
  if (await page.locator('input[type="password"]').count()) {
    await page.fill('input[type="password"]', PASSWORD)
    await page.keyboard.press('Enter')
    await page.waitForTimeout(2500)
  }
  await page.locator('nav >> text=搜索').first().click().catch(() => {})
  await page.waitForTimeout(600)
  await page.locator('button', { hasText: /^图片$/ }).first().click()
  await page.waitForTimeout(800)

  const out = { steps: [] }

  // 1. start a build
  const started = await clickIfThere(page, '建立图片索引')
  out.steps.push({ step: '建立图片索引', clicked: started, toast: await toastText(page) })

  // 2. the button is gone now (the build is running) -- ask again
  await page.waitForTimeout(1500)
  const statusText = await page.evaluate(() => {
    const m = document.body.innerText.match(/图片索引[^\n]*/)
    return m ? m[0] : ''
  })
  out.steps.push({ step: '状态栏', text: statusText })

  // 3. pause
  const paused = await clickIfThere(page, '暂停')
  out.steps.push({ step: '暂停', clicked: paused, toast: await toastText(page) })

  // 4. resume
  const resumed = await clickIfThere(page, '继续')
  out.steps.push({ step: '继续', clicked: resumed, toast: await toastText(page) })

  // 5. what the API says when a build is already running
  const raw = await page.evaluate(async () => {
    const token = localStorage.getItem('sfm_token') || localStorage.getItem('token')
    const r = await fetch('/api/images/index/start', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: 'Bearer ' + token } : {}) },
      body: JSON.stringify({}),
    })
    return { status: r.status, body: await r.text() }
  })
  out.startAgain = raw

  // 6. is there a way to stop it from the page?
  out.hasStopButton = await page.evaluate(() =>
    !!Array.from(document.querySelectorAll('button')).find((b) => /停止|取消/.test(b.textContent)))

  out.pageErrors = errors
  console.log(JSON.stringify(out, null, 2))
  await browser.close()
}

run().catch((e) => { console.error(e); process.exit(1) })
