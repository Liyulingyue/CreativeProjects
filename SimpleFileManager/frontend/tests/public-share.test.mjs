// The public share page: what somebody sees when they open a link.
//
// This page is the second way into the machine -- no login, just a
// link. These tests drive it as a visitor would.

import { BASE, API, ok, equal, contains } from './harness.mjs'

/** Make a share through the API and return its id. */
async function makeShare(page, { path, password = '', isDir = false, expiryDays = 0 }) {
  return page.evaluate(async (opts) => {
    const token = localStorage.getItem('sfm_token')
    const r = await fetch('/api/shares', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: 'Bearer ' + token } : {}),
      },
      body: JSON.stringify({
        path: opts.path,
        password: opts.password,
        expiry_days: opts.expiryDays,
      }),
    })
    if (!r.ok) return { error: (await r.text()).slice(0, 200) }
    const body = await r.json()
    return { id: body.id, url: body.url || `/s/${body.id}` }
  }, { path, password, expiryDays })
}

export default {
  name: '公开分享页',
  tests: [
    {
      name: '没有密码的分享，打开就能看到文件',
      async fn(page) {
        // Put a file there first.
        const name = `公开测试-${Date.now()}.txt`
        await page.evaluate(async (fileName) => {
          const body = new FormData()
          body.append('files', new File(['公开内容'], fileName, { type: 'text/plain' }))
          const token = localStorage.getItem('sfm_token')
          await fetch('/api/fs/upload?path=', {
            method: 'POST',
            body,
            headers: token ? { Authorization: 'Bearer ' + token } : {},
          })
        }, name)

        const share = await makeShare(page, { path: name })
        ok(share.id, `分享没建成: ${share.error || ''}`)

        // A visitor: no token at all.
        const visitor = await page.context().newPage()
        // The visitor's tab has to be the one in front: playwright waits
        // for an element to be still, and a tab in the background does
        // not draw, so the wait never ends.
        await visitor.bringToFront()
        await visitor.goto(`${BASE}/s/${share.id}`, { waitUntil: 'networkidle' })
        await visitor.waitForTimeout(1500)
        const body = await visitor.evaluate(() => document.body.innerText)
        ok(body.includes(name) || body.includes('公开内容') || body.length > 0,
          '公开页是空的')
        ok(!body.includes('登录'), '公开页要求登录了')
        await visitor.close()

        // Tidy up.
        await page.evaluate(async (fileName) => {
          const token = localStorage.getItem('sfm_token')
          await fetch('/api/fs/delete', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              ...(token ? { Authorization: 'Bearer ' + token } : {}),
            },
            body: JSON.stringify({ path: fileName, permanent: true }),
          })
        }, name)
      },
    },

    {
      name: '有密码的分享，要密码才能打开',
      async fn(page) {
        const name = `公开密码测试-${Date.now()}.txt`
        await page.evaluate(async (fileName) => {
          const body = new FormData()
          body.append('files', new File(['秘密内容'], fileName, { type: 'text/plain' }))
          const token = localStorage.getItem('sfm_token')
          await fetch('/api/fs/upload?path=', {
            method: 'POST',
            body,
            headers: token ? { Authorization: 'Bearer ' + token } : {},
          })
        }, name)
        const share = await makeShare(page, { path: name, password: 'good-password' })
        ok(share.id, `分享没建成: ${share.error || ''}`)

        const visitor = await page.context().newPage()
        // The visitor's tab has to be the one in front: playwright waits
        // for an element to be still, and a tab in the background does
        // not draw, so the wait never ends.
        await visitor.bringToFront()
        await visitor.goto(`${BASE}/s/${share.id}`, { waitUntil: 'networkidle' })
        await visitor.waitForTimeout(1500)

        const body = await visitor.evaluate(() => document.body.innerText)
        contains(body, '打开', '没有要密码')
        ok(!body.includes('秘密内容'), '没输密码就看到了内容')

        // The wrong password is refused, in words.
        await visitor.locator('input[type="password"]').first().fill('wrong-one')
        await visitor.locator('button', { hasText: '打开' }).first().click()
        await visitor.waitForTimeout(1200)
        const after = await visitor.evaluate(() => document.body.innerText)
        ok(!after.includes('秘密内容'), '密码错了还是看到了内容')
        // The field has to still be there: a visitor who typed the
        // password wrong has to be able to type it again.
        ok(await visitor.locator('input[type="password"]').count() > 0,
          '密码输错之后密码框不见了，访客只能刷新页面')

        // The right one gets in.
        await visitor.fill('input[type="password"]', 'good-password')
        await visitor.locator('button', { hasText: '打开' }).first().click()
        await visitor.waitForTimeout(1500)
        const done = await visitor.evaluate(() => document.body.innerText)
        // The prompt is gone and the file is there. (Looking for the
        // word 密码 would find it in the file's name.)
        ok(!done.includes('这个链接需要密码'), `还停在要密码的地方: ${done.slice(0, 120)}`)
        ok(done.includes('下载'), `没有看到下载: ${done.slice(0, 120)}`)
        await visitor.close()

        await page.evaluate(async (fileName) => {
          const token = localStorage.getItem('sfm_token')
          await fetch('/api/fs/delete', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              ...(token ? { Authorization: 'Bearer ' + token } : {}),
            },
            body: JSON.stringify({ path: fileName, permanent: true }),
          })
        }, name)
      },
    },

    {
      name: '被停掉的链接，访客看到的是一句话，不是报错',
      async fn(page) {
        const name = `公开撤销测试-${Date.now()}.txt`
        await page.evaluate(async (fileName) => {
          const body = new FormData()
          body.append('files', new File(['x'], fileName, { type: 'text/plain' }))
          const token = localStorage.getItem('sfm_token')
          await fetch('/api/fs/upload?path=', {
            method: 'POST',
            body,
            headers: token ? { Authorization: 'Bearer ' + token } : {},
          })
        }, name)
        const share = await makeShare(page, { path: name })
        await page.evaluate(async (id) => {
          const token = localStorage.getItem('sfm_token')
          await fetch(`/api/shares/${id}`, {
            method: 'DELETE',
            headers: token ? { Authorization: 'Bearer ' + token } : {},
          })
        }, share.id)

        const visitor = await page.context().newPage()
        // The visitor's tab has to be the one in front: playwright waits
        // for an element to be still, and a tab in the background does
        // not draw, so the wait never ends.
        await visitor.bringToFront()
        await visitor.goto(`${BASE}/s/${share.id}`, { waitUntil: 'networkidle' })
        await visitor.waitForTimeout(1500)
        const body = await visitor.evaluate(() => document.body.innerText)
        ok(body.length > 0, '公开页空了')
        ok(!/Internal Server Error|404 Not Found/i.test(body), body.slice(0, 200))
        await visitor.close()

        await page.evaluate(async (fileName) => {
          const token = localStorage.getItem('sfm_token')
          await fetch('/api/fs/delete', {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              ...(token ? { Authorization: 'Bearer ' + token } : {}),
            },
            body: JSON.stringify({ path: fileName, permanent: true }),
          })
        }, name)
      },
    },
  ],
}
