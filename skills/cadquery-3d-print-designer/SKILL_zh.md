# CadQuery 3D Print Designer（中文说明）

> 面向中文开发者的快速了解文档。SKILL.md 是给 AI 模型读取的执行规范，本文件是给你看的"它到底是什么 / 怎么用"。

## 这是什么

一个用于 **3D 打印参数化建模** **和** **就地修改拓竹/Orca 3MF 项目** 的工作流 skill，结合三个开源引擎，每个引擎只做自己最擅长的事：

| 引擎 | 职责 |
| --- | --- |
| **CadQuery** | 参数化 CSG 几何生成（螺丝柱、栅格、筋板、外壳） |
| **PyVista** | 基于 VTK 的离屏多视角渲染（PNG 预览） |
| **Trimesh + zipfile** | 3MF 解包/重新打包：读入拓竹 ZIP、字节级替换网格、原路打包回来，**不丢失拓竹特有的切片/彩印元数据** |

## 双流水线架构

工作流被刻意拆成 **两条相互独立的流水线**，按任务选一条，不要混用。

### 流水线 A —— 从零生成（CadQuery → STL → 预览）

```
参数定义 → CadQuery CSG 建模 → 导出 STL → PyVista 多视角渲染 PNG
                ↑                                    ↓
                └────────── 视觉评估 / 参数迭代 ──────┘
```

适用：用户手里没有 3MF，只有尺寸数据。

### 流水线 B —— 就地修改拓竹/Orca 的 .3mf（解包 → 改 → 打包）

```
input.3mf ──[trimesh 读]──> 几何 + 包围盒
           ──[zipfile 读]──> 全部元数据（彩印/打印参数/板块布局/...）
                                |
                                v
                       CadQuery CSG 几何
                                |
                                v
              [字节级 XML 替换，把新网格塞进 3dmodel.model]
                                |
                                v
output.3mf <─ 重新打包：除了改过的文件，原 ZIP 其余字节原样拷贝
```

适用：用户已经在拓竹/Orca 里有现成项目，调过色、改过打印预设、摆过板 —— 这些都不想丢。

**铁律**：永远不要 `trimesh.Scene.export("out.3mf")`。那会得到一个技术上合法的 3MF，但拓竹里那些彩印 XML / 切片参数全没了。必须解包 → 改 → 原路打包。

## 能干什么

典型应用场景：

- Mini PC / NUC / 树莓派 / ITX 机箱的**安装支架**
- 风扇罩、风道、转接板
- 外壳、壳体、自定义硬件固定件
- 在拓竹已有的 `.3mf` 上**就地加一个栅格 / 修改某个零件的网格 / 追加一个 Modifier**
- 任何"我说尺寸、你出模型 + 预览图"的参数化硬件

模型自动包含 3D 打印配合间隙：

- 螺丝孔 +0.2 ~ +0.3 mm
- 压配合槽
- 装配面

## 30 秒上手

### 1. 准备环境

```bash
python -m venv .venv
.venv\Scripts\Activate.ps1     # Windows

pip install cadquery pyvista trimesh
# 可选：布尔合并需要的后端
pip install manifold3d
```

### 2. 让 AI 用这个 skill

**从零建模**：

> "用 cadquery-3d-print-designer 给我做一个 80mm 风扇转 100mm 孔位的减震支架，4 个 M4 螺丝孔，中心距 71.5mm"

**修改已有的拓竹项目**：

> "用 cadquery-3d-print-designer 打开我的 input_bambu.3mf，给 object id=1 的零件加一个通风栅格，保留原有的彩印和打印参数"

AI 会按上面"双流水线"选择 A 还是 B，并按选定流水线执行。

### 3. 输出物

每次迭代你会在工作目录拿到：

| 流水线 | 文件 |
| --- | --- |
| A | `xxx.stl`、`preview_iso.png` / `preview_front.png` / `preview_top.png`、（无 PyVista 时）`preview_fallback.svg` |
| B | `xxx_modified.3mf`（拓竹切片软件直接打开，彩印/预设/板面布局全部保留） |

## 拓竹 3MF 的坑与避坑

1. **彩印是按顶点存的**。任何重网格化都会丢彩印。**带颜色的零件优先用 `add_modifier`**（不碰原网格）。
2. **打印参数在 `Metadata/project_settings.config`**。拓竹打开时会读，丢了会回到默认。我们的 round-trip 是字节级替换，原文件一字节不动。
3. **板面布局在 `Metadata/slice_info.config` / `Metadata/plate_1.json`**。同上一条。
4. **拓竹用额外命名空间**（`p:`、`bambu:`）。普通 XML 写入器会重写命名空间。我们做字节级 XML 替换，命名空间声明、属性顺序、空白全部原样保留。
5. **部分 3MF 把网格拆到 `3D/Objects/Object_N.model`**。当前实现只覆盖网格全在 `3dmodel.model` 里的常见情况。遇到 "object N 没有 mesh" 错误时，让用户在拓竹里 **File → Export → 3MF**（不要 Save）先展平。
6. **Modifier 要在两处设置**：XML 里 `type="other"`（我们会写）+ 拓竹界面里右键 → Type → Modifier（只能用户手动）。导入后还要手动点一下。
7. **布尔合并需要 manifold3d 后端**：trimesh 默认没装。`union_replace` 报 ModuleNotFoundError 就 `pip install manifold3d`，或者改用 `add_modifier`。

## 关键约定（写给开发者）

修改或扩展这个 skill 时：

1. **STL/3MF 网格必须是流形（manifold）**：不能有反面法线、非流形边。CadQuery 的 CSG 操作天然保证这一点，但 `.stl` 导入再导出可能破坏。
2. **打印间隙默认值**：M3 → 3.3 mm，M4 → 4.3 mm，M5 → 5.3 mm，压配合额外 +0.1 ~ +0.2 mm。
3. **PyVista 离屏渲染**：必须 `off_screen=True`，否则会弹 GUI。Linux 服务器需要 `pv.start_xvfb()`。
4. **回退方案**：检测不到 PyVista 时不要硬失败，切到 `cq.exporters.export(..., "xxx.svg", opt={...})`。
5. **3MF 写入严禁全量重生成**：必须解包 → 字节级改 → 重新打包。任何重写整个 3dmodel.model 的路径都会丢拓竹元数据。
6. **`generators/three_mf.py` 里 cadquery 是懒加载**：纯 trimesh 的读 / 检查 / round-trip 不依赖 CadQuery，方便 CI smoke test。

## 依赖项

- Python >= 3.9
- cadquery >= 2.4
- pyvista >= 0.40（可选，无则回退 SVG）
- trimesh >= 4.0
- numpy（trimesh 传递依赖）
- manifold3d（可选，仅 `union_replace` 需要）
- Linux 下额外：xvfb（PyVista 离屏渲染）

## 文件清单

| 文件 | 给谁看 |
| --- | --- |
| `SKILL.md` | 给 AI 看的执行规范（opencode 加载，**英文**，含完整代码样例） |
| `SKILL_zh.md` | 本文件，给开发者看的快速了解 |
| `generators/three_mf.py` | 3MF round-trip 引擎（trimesh + 字节级 XML + zipfile） |
| `generators/three_mf_selftest.py` | 3MF round-trip 自测 |
| `generators/core.py` | 栅格排布 / 流形校验 / 解析体积公式 |
| `generators/grid_panel.py` | 单向穿孔板生成器 |
| `generators/ribbed_plate.py` | 双向栅格（带基板、永为单体） |
| `generators/render.py` | PyVista 多视角渲染 |
| `generators/selftest.py` | CadQuery 生成器自测 |