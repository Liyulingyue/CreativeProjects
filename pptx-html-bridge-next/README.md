# pptx-html-bridge-next

`pptx-html-bridge` 的架构重构版：**纯 Rust，零 Python 运行时依赖**，单二进制。

## 架构：manifest 是唯一信源

```
PPTX ──read──▶ manifest.json（类型化信源） ──write──▶ PPTX
                    │
                    └──view──▶ HTML（纯渲染，只给人看，永不被解析回读）
```

旧版（../pptx-html-bridge/）的解析折磨，一半来自 OOXML 规格本身，
一半来自"HTML 既给人看又给机器解析"的双重职责。新版把信源拆成
类型化 JSON（serde，deny_unknown_fields——字段拼错编译期就报），
HTML 降级为纯视图。

## 命令

```bash
phb read    deck.pptx outdir/    # PPTX → manifest.json + assets/ + 保真度报告
phb write   manifest.json out.pptx
phb view    manifest.json outdir/        # 需先 read（assets 同目录）
phb roundtrip deck.pptx outdir/          # 读→写→再读，验证稳定性
```

## v0.1 覆盖范围（实测）

| 能力 | 说明 |
|---|---|
| 文本框 | 多段落、多 run（字号/加粗/斜体/下划线/颜色/字体，latin+ea 同写）、对齐、缩进级别、行距、word_wrap、autofit、垂直锚点 |
| 形状 | prstGeom 原名往返、纯色填充/无填充、边框（颜色/宽度/虚线）、旋转、阴影（outerShdw 全参数）、形状内文字 |
| 图片 | 位置尺寸、srcRect 百分比裁切、旋转；**内容哈希命名**——同一张图跨轮次同名，自动去重 |
| 连接线 | 颜色/宽度/虚线、flipH/V、箭头尾端（tailEnd） |
| 表格 | 网格列宽/行高、纯文本单元格；合并单元格记 lossy |
| 主题色 | schemeClr → RGB（含 lumMod/lumOff 近似），读取时折算，写回显式 RGB |
| 组合形状 | 拍平为绝对定位元素（chOff/chExt 变换已处理），保真度报告注明 |
| 备注 / 图表 / SmartArt | v0.1 记为 unsupported 占位（视觉留痕），完整支持待 v0.2 |

## 保真度报告

每次 `read` 输出逐页的损益清单，例如：

```
· 第 7 页：组合形状已拍平（位置保留，分组关系丢失）
· theme/scheme 颜色已折算为 RGB
```

`roundtrip` 命令额外做**往返稳定性验证**：读→写→再读，两轮
manifest 逐字段对比——实测 26 页真 deck（含 46 条线、22 个箭头、
7 张截图）STABLE。

## 测试

```bash
cargo test
```

三个测试：无图片往返全等、带图片内容哈希命名、未知字段报错建议
（colour → color 前缀匹配）。

## 与旧版（../pptx-html-bridge/）的关系

- 旧版保留不动；新版是架构级重构，不是补丁
- 新版没有 HTML→PPTX 的"解析 HTML"路径——编辑发生在 manifest.json 上
  （类型化、校验严格），HTML 只是渲染产物
- 旧版尚未支持的 **shadow 往返**，新版已支持
- OOXML 承重层（包模型/rels/theme）由嵌入的空白模板打底（开发期用
  python-pptx 引导生成一次，运行时零 Python）
