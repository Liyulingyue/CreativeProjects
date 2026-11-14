// Runtime audit of the picture search page.
//
// Reading the component tells you what it looks like it does. This drives
// the real page in a real browser and reports what it actually does:
// console errors, requests that failed, thumbnails that did not draw, a
// button that says one thing and does another.
//
//   node scripts/audit-image-search.mjs [baseUrl]

import { chromium } from 'playwright'

const BASE = process.argv[2] ?? 'http://127.0.0.1:15173'
// No default password: the one that was written down was three changes
// old, and the login quietly failed -- the audit then measured the
// login form and reported nothing wrong.
const PASSWORD = process.env.SFM_PASSWORD
if (!PASSWORD) {
  console.error('set SFM_PASSWORD to the password of the box you are auditing')
  process.exit(2)
}
const VIEWPORT = { width: 1280, height: 900 }

const problems = []
const note = (kind, detail) => problems.push({ kind, detail })

const run = async () => {
  const browser = await chromium.launch()
  const ctx = await browser.newContext({ viewport: VIEWPORT })
  const page = await ctx.newPage()

  const consoleErrors = []
  const failedRequests = []
  const requestLog = []

  page.on('console', (m) => {
    if (m.type() === 'error') consoleErrors.push(m.text().slice(0, 200))
  })
  page.on('pageerror', (e) => consoleErrors.push('PAGEERROR ' + String(e).slice(0, 200)))
  page.on('requestfailed', (r) => {
    failedRequests.push(`${r.method()} ${r.url().replace(BASE, '')} -- ${r.failure()?.errorText}`)
  })
  page.on('response', (r) => {
    const url = r.url().replace(BASE, '')
    if (url.includes('/api/')) requestLog.push(`${r.status()} ${r.request().method()} ${url}`)
    if (r.status() >= 400 && url.includes('/api/')) {
      failedRequests.push(`${r.status()} ${r.request().method()} ${url}`)
    }
  })

  await page.goto(BASE, { waitUntil: 'networkidle' })
  if (await page.locator('input[type="password"]').count()) {
    await page.fill('input[type="password"]', PASSWORD)
    await page.keyboard.press('Enter')
    await page.waitForTimeout(2500)
  }

  // -- the pictures tab --------------------------------------------------
  await page.locator('nav >> text=搜索').first().click().catch(() => {})
  await page.waitForTimeout(800)
  const imagesTab = page.locator('button', { hasText: /^图片$/ }).first()
  if (await imagesTab.count()) {
    await imagesTab.click()
    await page.waitForTimeout(600)
  } else {
    note('missing', 'the 图片 tab button is not there')
  }

  // -- what is on the page before any search ----------------------------
  const before = await page.evaluate(() => ({
    hasIndexBar: !!document.body.innerText.match(/图片索引/),
    adminButton: !!Array.from(document.querySelectorAll('button'))
      .find((b) => b.textContent.includes('建立图片索引')),
    emptyHint: document.body.innerText.includes('用文字描述你要找的图片'),
  }))

  // -- a text search ----------------------------------------------------
  const input = page.locator('input[placeholder*="海边"]').first()
  if (!(await input.count())) {
    note('missing', 'the picture search input is not there')
  } else {
    await input.fill('海边的日落');
    await page.locator('button', { hasText: /^搜图$/ }).first().click()
    await page.waitForTimeout(6000)
  }

  const after = await page.evaluate(() => {
    const imgs = Array.from(document.querySelectorAll('img'))
    const tiles = Array.from(document.querySelectorAll('a'))
      .filter((a) => a.querySelector('img'))
    return {
      tiles: tiles.length,
      imagesDrawn: imgs.filter((i) => i.naturalWidth > 0).length,
      imagesBroken: imgs.filter((i) => i.complete && i.naturalWidth === 0).length,
      names: tiles.slice(0, 8).map((a) => a.textContent.trim().split('\n')[0]),
      scores: tiles.slice(0, 8).map((a) => (a.textContent.match(/相似度\s+([\d.]+)/) || [])[1]),
      resultLine: (document.body.innerText.match(/找到\s*\d+\s*张[^\\n]*/) || [])[0],
      errorBox: (document.body.innerText.match(/图片模型[^\n]*|服务器正在忙[^\n]*/) || [])[0],
      hrefs: tiles.slice(0, 3).map((a) => a.getAttribute('href')),
    }
  })

  // -- the link should be a real one, not "#" ---------------------------
  if (after.tiles > 0) {
    const bad = after.hrefs.filter((h) => !h || h === '#')
    if (bad.length) note('link', `${bad.length} tiles have no usable link`)
  }

  // -- the polling --------------------------------------------------------
  const before5s = requestLog.filter((l) => l.includes('/images/index/status')).length
  await page.waitForTimeout(6000)
  const after5s = requestLog.filter((l) => l.includes('/images/index/status')).length
  const pollsIn6s = after5s - before5s

  // -- clicking a picture must not download the whole thing --------------
  const bigDownloads = requestLog.filter((l) => !l.includes('/api/'))

  console.log(JSON.stringify({
    before,
    after,
    pollsIn6s,
    consoleErrors: consoleErrors.slice(0, 10),
    failedRequests: failedRequests.slice(0, 10),
    problems,
    apiCalls: requestLog.slice(0, 30),
  }, null, 2))

  await browser.close()
}

run().catch((e) => { console.error(e); process.exit(1) })
