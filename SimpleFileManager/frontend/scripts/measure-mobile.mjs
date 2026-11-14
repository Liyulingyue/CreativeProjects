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
// No default password on purpose: the one that was written here was
// three changes old, so the login failed and every page below was
// quietly measured on the login form. A tool that cannot see anything
// and says "nothing is wrong" is worse than no tool.
const PASSWORD = process.env.SFM_PASSWORD
if (!PASSWORD) {
  console.error('set SFM_PASSWORD to the password of the box you are measuring')
  process.exit(2)
}
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
// Each view is reached by its own address. Clicking the sidebar's items
// does not work on a phone: the sidebar is collapsed there, so the items
// carry icons but no words, and the click finds nothing. The click was
// wrapped in a catch, so every measurement below was quietly taken on
// the page that happened to be open -- the same page five times.
const PATHS = {
  files: '/',
  chat: '/chat',
  search: '/search',
  index: '/index',
  settings: '/settings',
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
    // A drawer that is closed sits off the left of the screen on
    // purpose. It is not "bleeding" anywhere.
    const offScreen = r.right <= 0 || r.left >= vw
    if (!scrollsX && !offScreen && (r.right > vw + 1 || r.left < -1)) {
      problems.overflow.push({
        el: describe(el),
        left: Math.round(r.left),
        right: Math.round(r.right),
        by: Math.round(Math.max(r.right - vw, -r.left)),
      })
    }

    // Content wider than the box it lives in, with no way to scroll to it.
    const hidesX = cs.overflowX === 'hidden' || cs.overflowX === 'clip'
    // An element with "truncate" on it is meant to be cut short: the
    // ellipsis is the point. Only real clipping is a problem.
    const meantToBeShort = (el.className || '').toString().includes('truncate')
    if (hidesX && !meantToBeShort && el.scrollWidth > el.clientWidth + 1) {
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
    if (await page.locator('input[type="password"]').count()) {
      throw new Error('the login did not work -- every page below would be ' +
        'measured on the login form')
    }
  }

  const out = {}
  for (const [key] of TABS) {
    await page.goto(BASE + PATHS[key], { waitUntil: 'networkidle' })
    await page.waitForTimeout(1200)
    out[key] = await page.evaluate(audit)
    // The address has to be the one we asked for: a page that redirects
    // would be measured under the wrong name. The first words of the
    // page say what was really measured -- a script that silently
    // measures the same page five times looks exactly like one that
    // found nothing wrong.
    out[key].at = new URL(page.url()).pathname
    out[key].page = await page.evaluate(() =>
      (document.body.innerText || '').trim().split('\n').slice(0, 2).join(' / ').slice(0, 60))
  }

  console.log(JSON.stringify(out, null, 2))
  await browser.close()
}

run().catch((e) => { console.error(e); process.exit(1) })