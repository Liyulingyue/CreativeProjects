# CadQuery 3D Print Designer（中文说明）

> 面向中文开发者的快速了解文档。SKILL.md 是给 AI 模型读取的执行规范，本文件是给你看的"它到底是什么 / 怎么用"。

## 这是什么

一个用于 **3D 打印参数化建模** 的工作流 skill，结合两个开源 Python 库：

| 库 | 作用 |
| --- | --- |
| **CadQuery** | 通过 CSG（构造实体几何）操作生成可 3D 打印的 STL 模型，支持参数化驱动 |
| **PyVista** | 基于 VTK 的离屏多视角渲染，生成高质量预览图（PNG） |

如果环境里没有 PyVista，会自动回退到 CadQuery 自带的 SVG 矢量线框导出（零依赖方案）。

## 能干什么

典型应用场景：

- Mini PC / NUC / 树莓派 / ITX 机箱的**安装支架**
- 风扇罩、风道、转接板
- 外壳、壳体、自定义硬件固定件
- 任何"我说尺寸、你出模型 + 预览图"的参数化硬件

模型自动包含 3D 打印配合间隙：

- 螺丝孔 +0.2 ~ +0.3 mm
- 压配合槽
- 装配面

## 工作流（流水线）

```
参数定义 → CadQuery CSG 建模 → 导出 STL → PyVista 多视角渲染 PNG
                ↑                                    ↓
                └────────── 视觉评估 / 参数迭代 ──────┘
```

每次循环都会渲染多张 PNG（等角 / 正视 / 顶视），方便你对照设计意图迭代参数。

## 30 秒上手

### 1. 准备环境

```bash
python -m venv .venv
# Windows
.venv\Scripts\Activate.ps1
# Linux / macOS
source .venv/bin/activate

pip install cadquery pyvista
```

### 2. 让 AI 用这个 skill

在 opencode 里直接说类似：

> "用 cadquery-3d-print-designer 给我做一个 80mm 风扇转 100mm 孔位的减震支架，4 个 M4 螺丝孔，中心距 71.5mm"

AI 会自动加载该 skill 并按上面的流水线执行。

### 3. 输出物

每次迭代你会在工作目录拿到：

- `xxx.stl` —— 切片软件用的实体模型
- `preview_iso.png` / `preview_front.png` / `preview_top.png` —— 多视角预览图
- （无 PyVista 时）`preview_fallback.svg` —— 矢量线框

## 关键约定（写给开发者）

如果你要修改或扩展这个 skill，注意以下点：

1. **STL 必须是流形（manifold）**：不能有反面法线、非流形边，否则切片软件会报错。CadQuery 的 CSG 操作天然保证这一点，但 `.stl` 导入再导出可能破坏。
2. **打印间隙默认值**：M3 → 3.3 mm，M4 → 4.3 mm，M5 → 5.3 mm，压配合额外 +0.1 ~ +0.2 mm。
3. **PyVista 离屏渲染**：必须 `off_screen=True`，否则会弹 GUI 窗口打断流水线。Linux 服务器需要 `pv.start_xvfb()`。
4. **回退方案**：检测不到 PyVista 时不要硬失败，切到 `cq.exporters.export(..., "xxx.svg", opt={...})`，仍然能给用户视觉反馈。

## 依赖项

- Python >= 3.9
- cadquery >= 2.4
- pyvista >= 0.40（可选，无则回退 SVG）
- Linux 下额外：xvfb（PyVista 离屏渲染）

## 文件清单

- `SKILL.md` —— 给 AI 看的，opencode 加载的规范文件（英文）
- `SKILL_zh.md` —— 本文件，给开发者看的快速了解