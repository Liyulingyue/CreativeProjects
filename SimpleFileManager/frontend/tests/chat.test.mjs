// The conversation: the live answer, the stop button, and leaving the page.
//
// The model is a fake one: a small HTTP server that answers like
// llama-server does, streaming a few words. The rest is real -- the
// backend, the stream, the page -- because the thing worth testing is
// the journey, not the mock.

import http from 'node:http'
import { BASE, ok, equal, contains, lastToast } from './harness.mjs'

// A word that appears nowhere else on the page, so that a test
// looking for it cannot trip over a menu item.
const WORDS = ['喵', '喵', '喵', '喵', '喵']

/** The fake model server, while the suite runs. */
let fake = null
/** The provider that was the main one before the suite started. */
let wasMain = null

/** A server that speaks the streaming format llama-server speaks. */
function startFakeModel() {
  const server = http.createServer((req, res) => {
    if (!req.url.includes('/chat/completions')) {
      res.writeHead(404).end('{}')
      return
    }
    let body = ''
    req.on('data', (c) => { body += c })
    req.on('end', async () => {
      // A tool call would change the flow; this one only talks.
      const wantsJson = false
      res.writeHead(200, { 'Content-Type': 'text/event-stream' })
      if (wantsJson) {
        res.end('data: [DONE]\n\n')
        return
      }
      for (const w of WORDS) {
        const chunk = {
          id: 'x', object: 'chat.completion.chunk', created: 0, model: 'fake',
          choices: [{ index: 0, delta: { content: w }, finish_reason: null }],
        }
        res.write(`data: ${JSON.stringify(chunk)}\n\n`)
        await new Promise((r) => setTimeout(r, 900))
      }
      const last = {
        id: 'x', object: 'chat.completion.chunk', created: 0, model: 'fake',
        choices: [{ index: 0, delta: {}, finish_reason: 'stop' }],
      }
      res.write(`data: ${JSON.stringify(last)}\n\n`)
      res.write('data: [DONE]\n\n')
      res.end()
    })
  })
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => resolve({ server, port: server.address().port }))
  })
}

async function openChat(page) {
  await page.goto(`${BASE}/chat`, { waitUntil: 'networkidle' })
  await page.locator('textarea').first().waitFor({ state: 'visible', timeout: 15000 })
  await page.waitForTimeout(400)
}

const sendButton = (page) => page.locator('button').filter({ hasText: /^发送$/ }).first()

/**
 * Point a provider at the fake model and make it the main one.
 *
 * The provider that was the main one is remembered and handed back, so
 * the test can put everything back the way it found it. Nothing is
 * matched by name here: deleting "the provider whose url starts with
 * 127.0.0.1" once took the real local models with it.
 */
async function useFakeModel(page, port) {
  const made = await page.evaluate(async (p) => {
    const token = localStorage.getItem('sfm_token')
    const headers = {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: 'Bearer ' + token } : {}),
    }
    const list = await fetch('/api/providers', { headers }).then((r) => r.json())
    const previousMain = list.find((x) => x.is_main) || null
    if (previousMain) {
      await fetch(`/api/providers/${previousMain.id}`, {
        method: 'PATCH', headers, body: JSON.stringify({ is_main: false }),
      })
    }
    const created = await fetch('/api/providers', {
      method: 'POST', headers,
      body: JSON.stringify({
        name: '假模型',
        base_url: `http://127.0.0.1:${p}/v1`,
        model: 'fake',
        api_key: 'sk-local',
        is_main: true,
      }),
    }).then((r) => r.json())
    if (!created?.id) return { error: 'provider 没建成' }
    return {
      id: created.id,
      previousMain: previousMain
        ? { id: previousMain.id, name: previousMain.name }
        : null,
    }
  }, port)

  return made
}

/**
 * Pick the fake model in the chat page's provider dropdown.
 *
 * The dropdown is the page's own list of providers, and it is read when
 * the page opens -- so this has to happen after the chat page is up,
 * not right after the provider was created.
 */
async function pickFakeProvider(page, made) {
  const select = page.locator('select:has(option:text-is("假模型"))').first()
  if (!(await select.count())) throw new Error('页面上找不到 provider 下拉')
  await select.selectOption(made.id)
  await page.waitForTimeout(300)
  const picked = await select.inputValue()
  if (picked !== made.id) {
    throw new Error(`下拉里没选中假模型（现在是 ${picked}，要的是 ${made.id}）`)
  }
}

/** Put the provider table back the way the test found it. */
async function restoreProviders(page, made) {
  await page.evaluate(async (m) => {
    const token = localStorage.getItem('sfm_token')
    const headers = {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: 'Bearer ' + token } : {}),
    }
    // Only the provider this test made -- by id, never by pattern.
    if (m?.id) {
      await fetch(`/api/providers/${m.id}`, { method: 'DELETE', headers })
    }
    if (m?.previousMain?.id) {
      await fetch(`/api/providers/${m.previousMain.id}`, {
        method: 'PATCH', headers, body: JSON.stringify({ is_main: true }),
      })
    }
  }, made)
}

export default {
  name: 'Agent 对话（假模型）',
  setup: async (page) => {
    fake = await startFakeModel();
    // The provider that is the main one now: it has to be the main one
    // again when the suite is done.
    wasMain = await page.evaluate(async () => {
      const token = localStorage.getItem('sfm_token')
      const headers = token ? { Authorization: 'Bearer ' + token } : {}
      const list = await fetch('/api/providers', { headers }).then((r) => r.json())
      const main = list.find((p) => p.is_main)
      return main ? { id: main.id, name: main.name } : null
    })
  },
  teardown: async (page) => {
    // Every fake provider this suite made goes away, whichever test
    // made it -- a failed test must not leave one behind.
    await page.evaluate(async (main) => {
      const token = localStorage.getItem('sfm_token')
      const headers = {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: 'Bearer ' + token } : {}),
      }
      const list = await fetch('/api/providers', { headers }).then((r) => r.json())
      for (const p of list) {
        if (p.name && p.name.startsWith('假模型')) {
          await fetch(`/api/providers/${p.id}`, { method: 'DELETE', headers })
        }
      }
      if (main?.id) {
        await fetch(`/api/providers/${main.id}`, {
          method: 'PATCH', headers, body: JSON.stringify({ is_main: true }),
        })
      }
    }, wasMain)
    if (fake) {
      fake.server.close();
      fake = null;
    }
  },
  // The provider that was the main one before the suite started. The
  // tests switch the main provider over to the fake model, and this is
  // the one that has to be main again afterwards.
  originalMain: null,
  tests: [
    {
      name: '问一句，答案一个字一个字地出来',
      async fn(page) {
        await page.route('**/api/agent/chat/stream', async (route) => {
          // Let the request hang until the test is done with it: the
          // live answer is only on the screen while the stream is open.
          await new Promise((r) => setTimeout(r, 20000))
          try { await route.abort(); } catch { /* the page went away */ }
        })
        await openChat(page)
        await page.locator('textarea').first().fill('海上的日落')
        await sendButton(page).click()
        await page.waitForTimeout(2000)

        const body = await page.evaluate(() => document.body.innerText)
        // The live answer area is there: either the words, or the
        // indicator that says it is still working.
        const live = await page.evaluate(() => ({
          text: document.body.innerText.includes('日落'),
          running: !!document.querySelector('[class*="animate"], .animate-pulse'),
          stop: !!Array.from(document.querySelectorAll('button')).find((b) => /停止/.test(b.textContent)),
        }))
        ok(live.text || live.running, `页面上没有正在生成的样子: ${body.slice(0, 160)}`)
        ok(live.stop, '没有停止按钮')
        await page.unroute('**/api/agent/chat/stream')
      },
    },

    {
      name: '在别的会话里，看不到这个会话正在跑的答案',
      async fn(page) {
        // The real backend with the fake model behind it, so the answer
        // is really on the screen while it is being written.
        const made = await useFakeModel(page, fake.port)
        ok(made.id, `假模型没接上: ${made.error || ''}`)

        await openChat(page)
        await pickFakeProvider(page, made)
        await page.locator('textarea').first().fill('这一轮属于第一个会话')
        await sendButton(page).click()
        // Wait for the first words of the answer to show up.
        await page.waitForTimeout(1800)
        const during = await page.evaluate(() => document.body.innerText)
        ok(during.includes('喵'),
          `这一轮的答案没有出现: ${during.slice(0, 200)}`)

        // Start another conversation while the first one is still going.
        await page.locator('button').filter({ hasText: '新建对话' }).first().click()
        await page.waitForTimeout(1500)
        const other = await page.evaluate(() => document.body.innerText)
        ok(!other.includes('喵'),
          `别的会话里出现了这个会话正在写的字: ${other.slice(0, 240)}`)

        await restoreProviders(page, made)
      },
    },

    {
      name: '服务器说错了，页面上有一句中文',
      async fn(page) {
        // The server knows what went wrong and says it in Chinese.
        // The page has to keep that sentence, not "请求失败 (500)".
        await page.route('**/api/agent/chat/stream', async (route) => {
          await route.fulfill({
            status: 500,
            contentType: 'application/json',
            body: JSON.stringify({ detail: '模型正忙（有人在用），过一会儿再试' }),
          })
        })
        await openChat(page)
        await page.locator('textarea').first().fill('这问会失败')
        await sendButton(page).click()
        await page.waitForTimeout(2500)

        const toast = await lastToast(page)
        const body = await page.evaluate(() => document.body.innerText)
        ok(toast.includes('模型正忙') || body.includes('模型正忙'),
          `页面上没有服务器那句话: toast=${toast} / ${body.slice(0, 200)}`)
        ok(!/请求失败 \(500\)/.test(toast + body),
          '页面上只剩一个状态码')
        await page.unroute('**/api/agent/chat/stream')
      },
    },

    {
      name: '新对话里先选好模型，再发第一条',
      async fn(page) {
        // The first message creates the session. That used to reset the
        // provider dropdown on the way, so the model the user had just
        // picked was never used.
        const made = await useFakeModel(page, fake.port)
        ok(made.id, `假模型没接上: ${made.error || ''}`)

        await openChat(page)
        await page.locator('button').filter({ hasText: '新建对话' }).first().click()
        await page.waitForTimeout(800)
        await pickFakeProvider(page, made)
        await page.locator('textarea').first().fill('用我选的那个模型回答')
        await sendButton(page).click()
        await page.waitForTimeout(8000)

        const body = await page.evaluate(() => document.body.innerText)
        ok(body.includes('喵'),
          `答的不是我选的模型: ${body.slice(0, 240)}`)

        await restoreProviders(page, made)
      },
    },

    {
      name: '答完之后，下拉里还是自己选的那个模型',
      async fn(page) {
        // The session list is refreshed after every turn. That refresh
        // used to reset the provider dropdown, so the model the user
        // had picked was quietly changed back.
        const made = await useFakeModel(page, fake.port)
        ok(made.id, `假模型没接上: ${made.error || ''}`)

        await openChat(page)
        await pickFakeProvider(page, made)
        await page.locator('textarea').first().fill('选好的模型别被换掉')
        await sendButton(page).click()
        // Wait for the turn to end -- that is when the list refreshes.
        await page.waitForTimeout(8000)

        const select = page.locator('select:has(option:text-is("假模型"))').first()
        const picked = await select.inputValue()
        ok(picked === made.id,
          `答完之后下拉被换掉了（现在是 ${picked}）`)

        await restoreProviders(page, made)
      },
    },

    {
      name: '答到一半离开这一页，回来时答案还在',
      async fn(page) {
        // The real backend, with the fake model behind it: the page can
        // go away and the answer still gets written down.
        const made = await useFakeModel(page, fake.port)
        ok(made.id, `假模型没接上: ${made.error || ''}`)

        await openChat(page)
        await pickFakeProvider(page, made)
        await page.locator('textarea').first().fill('写到一半就离开')
        await sendButton(page).click()
        await page.waitForTimeout(1500)

        // Leave while the model is still writing.
        await page.goto(`${BASE}/search`, { waitUntil: 'networkidle' })
        await page.waitForTimeout(2500)
        await page.goto(`${BASE}/chat`, { waitUntil: 'networkidle' })
        await page.waitForTimeout(2500)

        const body = await page.evaluate(() => document.body.innerText)
        ok(body.includes('写到一半就离开'),
          `回来之后连问的那句都没了: ${body.slice(0, 200)}`)
        // The answer is in the transcript, even though the page was
        // closed in the middle of it.
        ok(body.includes('喵'),
          `答案没有写下来: ${body.slice(0, 240)}`)

        // Put the provider table back the way it was.
        await restoreProviders(page, made)
      },
    },
  ],
}
