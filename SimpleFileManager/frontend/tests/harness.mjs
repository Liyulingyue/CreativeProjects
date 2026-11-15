// A small harness for browser tests.
//
// There is no test framework in this project, and there does not need to
// be one: playwright is already here, and a test that drives the real
// page in a real browser is worth more than a test that mounts a
// component in a pretend one. So the harness is a login, a page, and
// a handful of assertions.

import { chromium } from 'playwright'

export const BASE = process.env.SFM_BASE ?? 'http://127.0.0.1:15173'
export const PASSWORD = process.env.SFM_PASSWORD
export const API = process.env.SFM_API ?? 'http://127.0.0.1:8000'

if (!PASSWORD) {
  console.error('set SFM_PASSWORD to the password of the box you are testing')
  process.exit(2)
}

/** Log in and wait for the app to be ready. */
export async function login(page) {
  await page.goto(BASE, { waitUntil: 'networkidle' })
  if (await page.locator('input[type="password"]').count()) {
    await page.fill('input[type="password"]', PASSWORD)
    await page.keyboard.press('Enter')
    await page.waitForTimeout(2500)
  }
  if (await page.locator('input[type="password"]').count()) {
    throw new Error('the login did not work')
  }
}

/** Open the picture search: the search page, the pictures tab. */
export async function openPictureSearch(page) {
  await page.goto(`${BASE}/search`, { waitUntil: 'networkidle' })
  await page.waitForTimeout(600)
  const tab = page.locator('button', { hasText: /^图片$/ }).first()
  if (!(await tab.count())) throw new Error('the 图片 tab is not there')
  await tab.click()
  await page.waitForTimeout(600)
}

/** The pictures that are on the screen, as {name, score, drawn}. */
export async function tiles(page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll('a'))
      .filter((a) => a.querySelector('img'))
      .map((a) => ({
        name: (a.textContent || '').trim().split('\n')[0],
        score: ((a.textContent || '').match(/相似度\s+([\d.]+)/) || [])[1],
        drawn: a.querySelector('img').naturalWidth > 0,
        href: a.getAttribute('href'),
      })))
}

/** The text of the last toast, if there is one. */
export async function lastToast(page) {
  return page.evaluate(() => {
    const nodes = Array.from(document.querySelectorAll(
      '[role="status"], [role="alert"], [class*="toast"]'))
    const texts = nodes.map((n) => (n.textContent || '').trim()).filter(Boolean)
    return texts.length ? texts[texts.length - 1] : ''
  })
}

// -- assertions ----------------------------------------------------------

export class AssertionError extends Error {}

export function ok(value, message) {
  if (!value) throw new AssertionError(message || `expected something true, got ${JSON.stringify(value)}`)
}

export function equal(got, want, message) {
  if (got !== want) {
    throw new AssertionError(message || `expected ${JSON.stringify(want)}, got ${JSON.stringify(got)}`)
  }
}

export function contains(haystack, needle, message) {
  if (!String(haystack).includes(needle)) {
    throw new AssertionError(message || `expected ${JSON.stringify(String(haystack))} to contain ${JSON.stringify(needle)}`)
  }
}

/** Run the tests and print a summary. Returns the number of failures. */
export async function run(suites) {
  const browser = await chromium.launch()
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 } })
  const page = await ctx.newPage()
  const consoleErrors = []
  page.on('pageerror', (e) => consoleErrors.push(String(e).slice(0, 200)))
  page.on('console', (m) => {
    // A failed request is what a mocked test asks for; a JavaScript
    // error is not.
    if (m.type() === 'error' && !m.text().includes('401')
        && !m.text().includes('Failed to load resource')) {
      consoleErrors.push(m.text().slice(0, 200))
    }
  })

  await login(page)

  let passed = 0
  const failures = []
  for (const suite of suites) {
    console.log(`\n${suite.name}`)
    for (const t of suite.tests) {
      const before = consoleErrors.length
      try {
        await t.fn(page)
        passed += 1
        const extra = consoleErrors.slice(before)
        console.log(`  ✓ ${t.name}`)
        if (extra.length) console.log(`      (页面报错: ${extra.join(' | ')})`)
      } catch (e) {
        failures.push({ suite: suite.name, name: t.name, error: e })
        console.log(`  ✗ ${t.name}`)
        console.log(`      ${String(e.message || e).split('\n').slice(0, 5).join('\n      ')}`)
      }
    }
  }
  await browser.close()

  console.log(`\n${passed} passed, ${failures.length} failed`)
  return failures.length
}
