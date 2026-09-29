# Changelog

## 0.4.0 (2026-08-30)

### 图表 / SmartArt / 自由形状 / 切换动画

- **图表（Chart）**：渲染为结构化数据表（`data-chart-type` + `.chart-data` 表格），
  反向转换重建为原生 PPTX 图表（支持 column/bar/line/pie/doughnut/area）
- **自由形状（Freeform）**：custGeom 路径提取为内联 SVG polygon（含填充/描边），
  反向转换通过 `build_freeform` 重建
- **SmartArt**：渲染为可见占位框（不再静默消失）
- **切换动画**：fade 切换转 CSS 淡入动画（反向不支持，见已知限制）

### 反向转换增强（HTML → PPTX）

- 文本框旋转回写（`transform: rotate()` → `shape.rotation`）
- 超链接回写（`<a href>` → `run.hyperlink.address`）
- 视频海报帧回写（`<video poster>` → `add_movie(poster_frame_image=...)`）
- 演讲者备注回写（隐藏 `<div class="notes">` → notes_slide）
- 表格单元格背景色与合并单元格（rowspan/colspan → `cell.merge`）

### 正向转换增强（PPTX → HTML）

- **组合形状（GROUP）递归展开**：正确应用 chOff/chExt 子坐标系变换，支持嵌套组合
- **主题色文字提取**：scheme color 通过主题解析为具体 RGB（新增 `resolve_theme_color`）
- **表格样式**：列宽/行高（colgroup/tr height）、单元格填充、合并单元格（colspan/rowspan）、
  垂直对齐、单元格边距、单元格内文字样式
- **项目符号列表**：buChar/buAutoNum 还原（含编号格式 a. / 1. / (1) 等），
  支持 marL/indent 悬挂缩进
- **段落行距**：`paragraph.line_spacing`（倍数或固定值）转 CSS line-height
- **文本框旋转**：`transform: rotate()`
- **自动缩放**：normAutofit fontScale 应用到字号
- **阴影**：outerShdw 效果转 CSS box-shadow（含方向/距离/模糊/透明度）
- **超链接**：run 级超链接转 `<a href>`
- **演讲者备注**：输出为隐藏 `<div class="notes">`

### Bug 修复

- **目录批量转换互串**：多个 PPTX 转换到同一输出目录时 slides/media/索引相互覆盖。
  现在每个源文件输出到独立子目录 `{name}/`，`main.html` 链接相应更新
- **媒体文件名冲突**：幻灯片媒体文件名加入源文件名前缀（`{base}_slide{i}_img{n}.ext`）；
  版式/母版图片改用内容哈希命名，跨文件同图自动去重、异图绝不冲突

### 工程化

- 移除过期的 `setup.py`（元数据停留在 0.1.0 且缺少反向转换入口），
  统一由 `pyproject.toml` 管理，显式声明 `packages`
- `requires-python` 修正为 `>=3.8`（python-pptx 的实际要求）
- 新增 `tests/test_advanced.py`（12 用例）：组合形状、主题色、项目符号、
  超链接、备注、旋转、表格样式、目录转换隔离、反向回写

## 0.3.0 (2026-08-30)

- 实现 HTML → PPTX 反向转换，完成双向桥接
- 新增 `HTMLToPPTXConverter`、`convert_html_to_pptx` 与 CLI `html-to-pptx`
- 新增 css_utils / html_parsers / pptx_generators 模块
- round-trip 测试（文本样式、图片、表格、背景色、页面尺寸）

## 0.2.0 (2026-08-30)

- 移除背景图片字节的 `eval()` 还原，改为结构化返回
- 修复图片背景相对路径、图片填充类型判断（PICTURE=6）
- 新增渐变背景转 CSS linear-gradient
- 默认文字颜色按背景亮度选择
- 修复 PP_ALIGN 对齐映射、MSO_LINE_DASH_STYLE.DOT 不存在导致虚线失效、
  autoshape 被文本分支截胡、fonts.py 母版样式死代码
- HTML 文本转义防注入；layout 索引覆盖全部 master
- 新增 pytest 测试套件与演示样例生成脚本

## 0.1.0

- 初始版本：PPTX → HTML 单向转换
