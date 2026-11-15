# 前端测试

真浏览器里的测试：playwright 驱动真的页面，检查真的行为。

```bash
SFM_PASSWORD=你的密码 node tests/run.mjs
# 或者
npm test
```

需要前后端都在跑（`./run_dev.sh`）。

## 为什么不是 vitest

这个项目没有测试框架，也不需要装一个：playwright 已经在依赖里，
而且**在真浏览器里驱动真的页面**比在假 DOM 里挂一个组件更值得信。
框架只提供 `harness.mjs` 里的登录、几个断言和一个汇总。

## 有什么

| 文件 | 测什么 |
|---|---|
| `picture-search.test.mjs` | 搜图、排序、缩略图、找相似、服务器的错话 |
| `index-controls.test.mjs` | 建索引的按钮、管理员门控、「已经在跑了」 |
| `pages.test.mjs` | 设置页的图片索引卡片、文本搜索、Agent 对话 |

## 写测试时注意

- 状态是**轮询**的（5 秒一次）：改了 mock 之后要 `page.reload()`，
  否则屏幕上还是旧的状态。
- 用 `page.route()` 假装服务器说什么，用来测「服务器说忙」「不是管理员」
  这些平时很难造出来的情形。
