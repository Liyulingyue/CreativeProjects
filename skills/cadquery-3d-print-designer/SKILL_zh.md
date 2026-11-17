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
source .venv/bin/activate       # Linux / macOS

pip install cadquery pyvista trimesh
# 可选：布尔合并需要的后端
pip install manifold3d
```

**PyPI 慢的话换镜像** —— 光 `cadquery-ocp` 一个轮子就有 ~68MB，默认源上能卡好几分钟：

```bash
pip install -i https://pypi.tuna.tsinghua.edu.cn/simple cadquery pyvista trimesh manifold3d
```

**Linux 无头渲染**：PyVista 底层是 VTK，需要一个 GL context。两种方式：

1. **EGL / OSMesa（首选，不需要 root）**。`DISPLAY` 为空时 VTK 会自动回退，前提是系统装了 Mesa。Ubuntu 上是 `libegl1`、`libegl-mesa0`、`libgl1-mesa-dri`，用 `ldconfig -p | grep -E 'libEGL|libGL'` 验证。不用装别的东西。
2. **Xvfb（兜底）**。VTK 报缺 GL context 时才需要：`sudo apt install xvfb`，然后渲染前调 `pv.start_xvfb()`。

两种都是软件光栅化，做 CAD 预览完全够用，不需要 GPU。stderr 上会出现一行无害的 `bad X server connection. DISPLAY=` 警告，PNG 依然是正确的。

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

## 参考示例（examples/）

`examples/` 里是 6 个完整可跑的生成器。它们的作用是**参考实现** —— 每个演示一种建模手法 —— 而不是成品。
参数看各自模块的 docstring，下表只说明每个例子演示了什么技术：

| 示例 | 是什么 | 演示的技术 |
| --- | --- | --- |
| `examples/grid_panel.py` | 机箱穿孔板 | 栅格排布 + **解析体积**交叉校验 |
| `examples/ribbed_plate.py` | 双向栅格 | 如何保证格栅是**一个**实体 |
| `examples/hanger_hook.py` | 衣架钩尖保护套 | 沿曲线**扫掠**出型腔 |
| `examples/blade.py` | 一体成型乒乓球底板 | 宽度剖面 → **PCHIP** 样条，mitre 偏移做倒角 |
| `examples/paddle.py` | 快拆乒乓球拍 | **多零件装配体**：燕尾榫 + 双螺丝 |
| `examples/longcat_logo.py` | 美团 LongCat logo（三种构建） | SVG 路径解析、分色拆分、鼓起的 3D 头 |

```python
from examples.blade import table_tennis_blade, BladeSpec

blade = table_tennis_blade(BladeSpec())
blade.save("blade.stl")
print(blade)
# table_tennis_blade  volume   133,170.9 mm^3  faces 1927  mass  165.1 g
```

底板轮廓的做法是「宽度剖面」—— 一张"往下走多少、宽多少"的表 —— 用保形三次插值（PCHIP）算出来，
再按弧长重采样。两个坑记一下：

- **不要用 `edges(">Z").fillet(r)` 做边缘圆角**：OCCT 的 fillet 在面边环超过约 180 条边时
  会直接放弃，而光滑轮廓恰恰需要很多条边。改用布尔切：把轮廓向内做 mitre 偏移，
  用两个轮廓之间的环从上下两面切掉。
- **宽度剖面的采样点必须单调递增**：一旦乱序，PCHIP 的二分查找会静默出错，轮廓会自己折回去。

`pipeline.demo` 会把样例产物渲染到 `out/`；`pipeline.selftest` 验证所有生成器。

## 美团 LongCat logo（三种构建方式）

`examples/longcat_logo.py` 收录了**美团 LongCat logo** —— M 形外轮廓、双耳之间的 V 形缺口、圆润的猫脸、两只斜眼 —— 路径数据直接解析自真实 SVG，比例精确。三种构建共享同一套轮廓源：

```python
from examples.longcat_logo import longcat_logo, longcat_logo_split, longcat_logo_3d

longcat_logo().save("logo.stl")          # 一块平板，脸和眼是凹槽（换料/喷涂上色）

parts = longcat_logo_split()             # 三块平板，可分色打印
for name, part in parts.items():
    part.save(f"{name}.stl")

parts = longcat_logo_3d()                # 头是鼓起的实体，眼睛与头部表面齐平
parts["cat_head"].save("cat_head.stl")
```

| 构建 | 函数 | 什么时候用 |
| --- | --- | --- |
| 平板 | `longcat_logo()` | 单色打印，颜色靠换料或喷涂 |
| 分色 | `longcat_logo_split()` | 多色打印，独立 STL / MMU |
| 3D | `longcat_logo_3d()` | 头要鼓起来成为立体件 |

3D 头的侧轮廓是 **弧 – 直线 – 弧**：边缘的直线段正好等于 `thickness`（和 M 外壳同宽），两条弧可以自由鼓。直线边就是面部轮廓本身，所以与外壳相接处无缝、无悬空。

两个实现要点（扩展时请保留）：

* **不要用缩放球体做穹顶。** OCCT 球体网格在极点会留下 4 边裂缝，导出的 STL 不 watertight。`_dome` 用轮廓放样到一点；`_ellipsoid`（椭圆线框放样）是需要光滑帽檐时的 watertight 替代。
* **直边必须是未缩放的轮廓。** 把轮廓按质心缩放后放样，直边会比外壳开口宽，头就会和 M 撞在一起。直边保持 `thickness`，只有鼓出部分长大。

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

1. **STL/3MF 网格必须是流形（manifold）**：不能有反面法线、非流形边。CadQuery 的 CSG 操作天然保证这一点，但 `.stl` 导入再导出可能破坏。`pipeline.core.verify()` 会用 OCCT 的 BRep 合法性检查把这类问题挡下来。
2. **打印间隙默认值**：M3 → 3.3 mm，M4 → 4.3 mm，M5 → 5.3 mm，压配合额外 +0.1 ~ +0.2 mm。
3. **PyVista 离屏渲染**：必须 `off_screen=True`，否则会弹 GUI。Linux 无头机器优先靠 EGL/Mesa，`pv.start_xvfb()` 是兜底。
4. **回退方案**：检测不到 PyVista 时不要硬失败，切到 `cq.exporters.export(..., "xxx.svg", opt={...})`。
5. **3MF 写入严禁全量重生成**：必须解包 → 字节级改 → 重新打包。任何重写整个 3dmodel.model 的路径都会丢拓竹元数据。
6. **`generators/three_mf.py` 里 cadquery 是懒加载**：纯 trimesh 的读 / 检查 / round-trip 不依赖 CadQuery，方便 CI smoke test。
7. **渲染边的坑**：在 STL 上 `show_edges=True` 画的是**三角面片的边**，不是零件的特征边 —— 一块平板会渲染成一片小方格，看起来像几何错了，其实只是画法骗人。要真边线用 `extract_feature_edges(feature_angle=30)` 单独提取再叠加（渲染器的 `edges=30` 就是这么做的）。**注意 `add_mesh(feature_angle=...)` 没用** —— 在 PyVista 0.49 里它只管平滑着色的锐边拆分，跟边线叠加无关。

## 依赖项

- Python >= 3.9
- cadquery >= 2.4
- pyvista >= 0.40（可选，无则回退 SVG）
- trimesh >= 4.0
- numpy（trimesh 传递依赖）
- manifold3d（可选，仅 `union_replace` 需要）
- Linux 无头：EGL/Mesa（`libegl1`、`libegl-mesa0`、`libgl1-mesa-dri`）就够了；`xvfb` 只是兜底

## 文件清单

| 文件 | 给谁看 |
| --- | --- |
| `SKILL.md` | 给 AI 看的执行规范（opencode 加载，**英文**，含完整代码样例） |
| `SKILL_zh.md` | 本文件，给开发者看的快速了解 |
| `CONTRIBUTING.md` | 什么该放进 `examples/`、什么不该（写给想扩展的人） |
| `pipeline/core.py` | 栅格排布 / 流形校验 / 解析体积公式 |
| `pipeline/render.py` | PyVista 多视角渲染 |
| `pipeline/three_mf.py` | 3MF round-trip 引擎（trimesh + 字节级 XML + zipfile） |
| `pipeline/selftest.py` | CadQuery 生成器自测 |
| `pipeline/three_mf_selftest.py` | 3MF round-trip 自测 |
| `pipeline/demo.py` | 渲染样例零件到 `out/` |
| `pipeline/three_mf_demo.py` | 3MF round-trip 演示 |
| `examples/grid_panel.py` | 单向穿孔板生成器 |
| `examples/ribbed_plate.py` | 双向栅格（带基板、永为单体） |
| `examples/hanger_hook.py` | 衣架挂钩末端保护套 + 参考挂钩 |
| `examples/blade.py` | 一体成型乒乓球底板（PCHIP 轮廓 / 斜接偏移 / 圆角边缘） |
| `examples/paddle.py` | 快拆乒乓球拍（燕尾榫 + 双螺丝夹紧） |
| `examples/longcat_logo.py` | 美团 LongCat logo（平板 / 分色 / 3D 鼓头，三种构建） |

**两个目录的分工**：`pipeline/` 是这个 skill 真正的引擎（校验、渲染、3MF 往返），
`examples/` 是参考实现的零件生成器。两者都是顶层包，没有 `__init__.py`，
直接 `from examples.grid_panel import grid_panel` 这样导入。