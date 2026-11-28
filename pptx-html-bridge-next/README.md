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
类型化 JSON（serde，deny_unknown_fields——字段拼错立即报错并给
修正建议），HTML 降级为纯视图。

## 命令

```bash
phb read    deck.pptx outdir/    # PPTX → manifest.json + assets/ + 保真度报告
phb write   manifest.json out.pptx
phb view    manifest.json outdir/        # 需先 read（assets 同目录）
phb roundtrip deck.pptx outdir/          # 读→写→再读，验证稳定性
```

## v0.2 覆盖范围（实测）

| 能力 | 说明 |
|---|---|
| 文本框 | 多段落、多 run（字号/加粗/斜体/下划线/颜色/字体含 panose/charset/pitchFamily/cs/sym、kern/spc 字距、lang）、对齐、marL/indent 缩进、行距、段前/段后距、word_wrap、autofit（含 fontScale/lnSpcReduction）、垂直锚点、**内边距 lIns/tIns/rIns/bIns**、a:br 换行往返 |
| 形状 | prstGeom 原名 + **adj 调整值**（圆角等）、纯色填充（含 alpha）、**渐变填充（gsLst 停靠点 + 线性角度）**、边框（颜色/宽度/虚线/双端箭头）、旋转、阴影（outerShdw 全参数含 algn）、形状内文字、**anchor**、内边距、**占位符 ph@type/idx 与几何继承** |
| 图片 | 位置尺寸、srcRect 百分比裁切、旋转、**点击超链接**；**内容哈希命名**——同一张图跨轮次同名，自动去重 |
| 自由形状 | custGeom 完整往返：moveTo/lnTo/cubicBezTo/quadBezTo/arcTo/close，多子路径，各自坐标空间 |
| 连接线 | 颜色/宽度/虚线、flipH/V、**headEnd + tailEnd 双端箭头** |
| 超链接 | run 级 + 图片级；外部 URL（TargetMode=External）与内部跳页双向往返，tooltip 保留。**内部跳页记 manifest 页号（sldIdLst 顺序）**，与源文件 part 编号解耦 |
| 项目符号 | buNone/buChar/buAutoNum + buClr/buSzPct/buFont，marL/indent 悬挂缩进 |
| 表格 | 网格列宽/行高、**合并单元格**（gridSpan/rowSpan/vMerge/hMerge 全语义）、单元格底色/加粗/字号/颜色/对齐；旧版纯字符串单元格仍兼容 |
| 备注 | notesSlide 正文提取与写回（\n 分段），含 notesMaster/theme2 打包 |
| 主题 | typed 主题清单（12 槽位 + 主/副字体）回填 theme1.xml；**母版资源包**：原文件的 slideMaster/全部版式/raw 主题/defaultTextStyle/母版侧媒体按原编号整体携带 |
| 主题色 | schemeClr → RGB（含 lumMod/lumOff 近似、**sysClr lastClr 正确解析**） |
| 组合形状 | 拍平为绝对定位元素（grpSpPr xfrm 变换已处理） |
| 健壮性 | **重复 part 名的 zip 自动修复**（中心目录级解析，最后一条 wins）；unsupported（chart/SmartArt 等）以隐形标记形状往返，任意 deck 往返 STABLE |

## 往返稳定性（真实样本实测）

`roundtrip` 读→写→再读，两轮 manifest 逐字节对比：

| 样本 | 页数 | 结果 |
|---|---|---|
| 此芯官方 PPT 模板（真实 PowerPoint 文件：组合/占位符/sysClr 主题/重复媒体） | 3 | **STABLE** |
| 决赛路演 v1（pptxgenjs） | 20 | **STABLE** |
| 决赛路演 v2 | 24 | **STABLE** |
| 决赛路演 v3（26 页、46 线、22 箭头、7 截图） | 26 | **STABLE，全程无损** |

LibreOffice 渲染像素对比（≥24 灰阶差异占比）：v3 全 26 页 **0.000%**，
官方模板 3 页 0.000% / 0.000% / 0.016%（水印亚像素噪声）——**像素级还原**。

## 保真度报告

每次 `read` 输出损益清单，重复项自动聚合计数：

```
· theme/scheme 颜色已折算为 RGB（×10）
```

## 测试

```bash
cargo test    # 7 个测试
```

覆盖：全要素无图片往返逐字节稳定（文字/渐变/箭头/合并表格/自由形状/
链接/备注/主题/unsupported 标记）、带图片内容哈希命名、unsupported
隐形标记、备注与主题回读、未知字段报错建议（colour → color）、旧版
字符串单元格兼容、纵向合并网格位保持。

## 与旧版（../pptx-html-bridge/）的关系

- 旧版保留不动；新版是架构级重构，不是补丁
- 新版没有 HTML→PPTX 的"解析 HTML"路径——编辑发生在 manifest.json 上
  （类型化、校验严格），HTML 只是渲染产物
- OOXML 承重层（包模型/rels/theme）由嵌入的空白模板打底（开发期用
  python-pptx 引导生成一次，运行时零 Python）；源文件若自带母版栈则整体携带

## 语义归一说明

以下场景读侧会归一化（首次 write→read 后 manifest 与手写值可能不同，此后稳定）：

- `theme: None` 或部分槽位缺省 → 读回时总是填充实际主题值（读侧永远给出完整 theme）
- `bullet.kind = "char"` 且未给 `character` → 写出缺省 `•`，读回带出
- 空的 `Link`（既无 url 也无 slide）→ 写出时不生成超链接，读回为 `null`
- 形状 `anchor: None` → 写出 `ctr`，读回归一为 `None`（ctr 即缺省语义）
