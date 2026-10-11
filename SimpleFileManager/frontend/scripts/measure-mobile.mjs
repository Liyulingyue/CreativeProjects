// Measure real layout at phone widths.
//
// Reading class names only tells you what a breakpoint is *supposed* to do.
// This loads each page in a real 375px viewport and reports what actually
// overflows, what gets clipped, and how small the tap targets really are --
// which is the part that "looks cramped" actually comes from.
//
//   node scripts/measure-mobile.mjs [baseUrl]

import { chromium } from 'playwright'

const BASE = process.argv[2] ?? 'http://127.0.0.1:15173'
const PASSWORD = process.env.SFM_PASSWORD ?? '20220924'
const VIEWPORT = { width: 375, height: 812 }

const TABS = [
  ['files', '文件'],
  ['chat', '对话'],
  ['search', '搜索'],
  ['index', '索引'],
  ['settings', '设置'],
]

// Each view is reached the same way the sidebar does: click its nav item.
// Must match the sidebar's own labels exactly; a mismatch silently leaves the
// previous view on screen and every measurement below is then the same page.
const NAV_LABEL = {
  files: '文件管理',
  chat: 'Agent 对话',
  search: '搜索',
  index: '索引管理',
  settings: '设置',
}

const audit = () => {
  const vw = window.innerWidth
  const problems = { overflow: [], clipped: [], smallTaps: [] }

  const describe = (el) => {
    const cls = (el.className || '').toString().trim().split(/\s+/).slice(0, 4).join(' ')
    const txt = (el.textContent || '').trim().slice(0, 24)
    return `${el.tagName.toLowerCase()}${el.className ? '.' + cls : ''}${txt ? ` «${txt}»` : ''}`
  }

  for (const el of document.querySelectorAll('body *')) {
    const r = el.getBoundingClientRect()
    if (r.width === 0 || r.height === 0) continue
    const cs = getComputedStyle(el)
    if (cs.visibility === 'hidden' || cs.display === 'none') continue

    // Horizontal bleed past the viewport. Ignore nodes that scroll internally.
    const scrollsX = cs.overflowX === 'auto' || cs.overflowX === 'scroll'
    if (!scrollsX && (r.right > vw + 1 || r.left < -1)) {
      problems.overflow.push({
        el: describe(el),
        left: Math.round(r.left),
        right: Math.round(r.right),
        by: Math.round(Math.max(r.right - vw, -r.left)),
      })
    }

    // Content wider than the box it lives in, with no way to scroll to it.
    const hidesX = cs.overflowX === 'hidden' || cs.overflowX === 'clip'
    if (hidesX && el.scrollWidth > el.clientWidth + 1) {
      problems.clipped.push({
        el: describe(el),
        scrollW: el.scrollWidth,
        clientW: el.clientWidth,
        by: el.scrollWidth - el.clientWidth,
      })
    }

    // Tap targets. 44px is the accessibility floor; 32px is the tolerable floor.
    const interactive =
      el.tagName === 'BUTTON' || el.tagName === 'A' || el.getAttribute('role') === 'button' ||
      el.getAttribute('role') === 'switch' || (el.tagName === 'INPUT' && el.type !== 'hidden')
    if (interactive && (r.height < 32 || r.width < 24)) {
      problems.smallTaps.push({
        el: describe(el),
        w: Math.round(r.width),
        h: Math.round(r.height),
      })
    }
  }

  return {
    vw,
    docScrollW: document.documentElement.scrollWidth,
    horizontalPageScroll: document.documentElement.scrollWidth > vw + 1,
    overflow: problems.overflow.slice(0, 12),
    clipped: problems.clipped.slice(0, 12),
    smallTaps: problems.smallTaps.slice(0, 12),
  }
}

const run = async () => {
  const browser = await chromium.launch()
  const ctx = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 2, isMobile: true, hasTouch: true })
  const page = await ctx.newPage()

  await page.goto(BASE, { waitUntil: 'networkidle' })

  // Log in if the login screen is up.
  if (await page.locator('input[type="password"]').count()) {
    await page.fill('input[type="password"]', PASSWORD)
    await page.keyboard.press('Enter')
    await page.waitForTimeout(2500)
  }

  const out = {}
  for (const [key, label] of TABS) {
    const nav = page.locator(`nav >> text=${label}`).first()
    if (await nav.count()) {
      await nav.click({ timeout: 5000 }).catch(() => {})
      await page.waitForTimeout(1200)
    }
    out[key] = await page.evaluate(audit)
  }

  console.log(JSON.stringify(out, null, 2))
  await browser.close()
}

run().catch((e) => { console.error(e); process.exit(1) })