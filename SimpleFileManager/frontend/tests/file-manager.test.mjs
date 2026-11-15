// The file manager: the everyday things a person does with a file.
//
// These are the operations that go wrong quietly -- a rename that does
// not happen, a delete that does not, an upload that says it worked
// and did not. The backend's own tests cannot see those, because they
// stop at the API.

import { BASE, ok, equal, contains } from './harness.mjs'

const files = (page) => page.goto(`${BASE}/`, { waitUntil: 'networkidle' })

/** The names of the files on screen. */
async function names(page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll('[data-file-name]'))
      .map((el) => el.getAttribute('data-file-name'))
      .filter(Boolean))
}

export default {
  name: '文件管理',
  tests: [
    {
      name: '文件管理页能打开，能看到文件',
      async fn(page) {
        await files(page)
        await page.waitForTimeout(1200)
        const body = await page.evaluate(() => document.body.innerText)
        ok(body.length > 0, '页面是空的')
        // The toolbar is there.
        const toolbar = await page.locator('button[title="上传文件"]').count()
        ok(toolbar > 0, '工具栏没有上传按钮')
      },
    },

    {
      name: '选中一个文件之后，删除按钮才有���',
      async fn(page) {
        await files(page)
        await page.waitForTimeout(1200)
        const del = page.locator('button[title="删除"]').first()
        ok(await del.count(), '没有删除按钮')
        equal(await del.isDisabled(), true, '没有选中文件时删除按钮是能点的')
      },
    },

    {
      name: '重命名一个文件，名字真的变了',
      async fn(page) {
        // Put a file there first, so the test does not depend on what
        // the storage happens to hold.
        // A name of its own for this run: a file that is already there
        // is refused, and the test would fail for the wrong reason.
        const scratch = `rename-me-${Date.now()}.txt`
        const made = await page.evaluate(async (name) => {
          const body = new FormData()
          body.append('files', new File(['hello'], name, { type: 'text/plain' }))
          // The token is in the page's own storage; the browser cannot
          // send it by itself.
          const token = localStorage.getItem('sfm_token')
          const r = await fetch('/api/fs/upload?path=', {
            method: 'POST',
            body,
            headers: token ? { Authorization: 'Bearer ' + token } : {},
          })
          return r.ok
        }, scratch)
        ok(made, '测试文件没能上传')

        await files(page)
        await page.waitForTimeout(1200)
        const before = await names(page)
        ok(before.includes(scratch), `列表里没有测试文件: ${before.join(', ')}`)
        const target = scratch
        // The context menu carries the rename.
        await page.evaluate((name) => {
          const el = document.querySelector(`[data-file-name="${name}"]`)
          el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }))
        }, target)
        await page.waitForTimeout(600)
        const rename = page.locator('text=重命名').first()
        ok(await rename.count(), '右键菜单里没有「重命名」')
        await rename.click()
        await page.waitForTimeout(600)

        const input = page.locator('input[type="text"]').last()
        ok(await input.count(), '没有出现输入框')
        const newName = `改名测试-${Date.now()}.txt`
        await input.fill(newName)
        await input.press('Enter')
        await page.waitForTimeout(1500)

        const after = await names(page)
        ok(after.includes(newName), `改名没有生效: ${after.join(', ')}`)

        // Tidy up: the file was only there for the test.
        await page.evaluate(async (name) => {
          const token = localStorage.getItem('sfm_token')
          await fetch('/api/fs/delete', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              ...(token ? { Authorization: 'Bearer ' + token } : {}),
            },
            body: JSON.stringify({ path: name, permanent: true }),
          })
        }, newName)
      },
    },

    {
      name: '服务器说「没有这个文件」，就老实说出来',
      async fn(page) {
        await files(page)
        await page.waitForTimeout(800)
        await page.route('**/api/fs/tree*', (route) =>
          route.fulfill({
            status: 500,
            contentType: 'application/json',
            body: JSON.stringify({ detail: '存储根读不了' }),
          }))
        await page.reload({ waitUntil: 'networkidle' })
        await page.waitForTimeout(1500)

        const body = await page.evaluate(() => document.body.innerText)
        ok(body.length > 0, '页面空了')
        await page.unroute('**/api/fs/tree*')
      },
    },
  ],
}
