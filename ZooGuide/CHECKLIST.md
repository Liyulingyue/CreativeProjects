# ZooGuide 验收 Checklist（明早 8:00）

## 启动流程

```bash
# 1. 后端 (在终端1)
cd /home/liyulingyue/Codes/CreativeProjects/ZooGuide/Backend
source .venv/bin/activate
python run.py
# → http://localhost:8000

# 2. 前端 (在终端2)
cd /home/liyulingyue/Codes/CreativeProjects/ZooGuide/Web/PWA
npm run dev
# → http://localhost:5173

# 一键启动（推荐）
cd /home/liyulingyue/Codes/CreativeProjects/ZooGuide
./start.sh
```

## 演示路径

### 1. 极速模式（< 1 秒出方案）
1. 打开 http://localhost:5173
2. 不勾选"⚡ 极速模式"或勾选（看场景）
3. 5 步问卷 → 路线展示
4. 看「讲解词」+「温馨提示」+「总时长统计」

### 2. LLM 模式（30-90 秒，讲解个性化）
1. 不勾选"⚡ 极速模式"
2. 同样流程，等待加载（页面有提示）
3. 比较 LLM 讲解词 vs 极速模式讲解词：
   - 年轻人对大熊猫的讲解 vs 带娃家长对大熊猫的讲解

### 3. 动态调整（60-120 秒）
1. 在路线页面点「✨ 动态调整」
2. 选择"走不动了，能少走点吗？" 或自定义反馈
3. 看后半段路线如何调整

### 4. 动物打卡
1. 在路线页面点「🦒 打卡」
2. 标记为"我在这里"
3. 切换不同场馆

### 5. GPS 自动定位（推荐演示）
1. 在路线页面点「🛰️ 自动定位当前位置」
2. 浏览器会请求位置权限（首次需要同意）
3. 自动跳转到最近的 stop
4. **演示技巧**：用手机浏览器访问 `http://<你的IP>:5173` 可以用真实GPS；桌面端可手动改浏览器定位（DevTools → Sensors → Location 设为 32.1030, 118.8100 = 大熊猫馆位置）

### 5b. 对比路线
1. 在路线页面点「🧭 对比」按钮
2. 弹出 3 条对比路线：必看精选 / 小众探索 / 平衡推荐
3. 点任意一条 → 切换到该路线

### 5c. 聊天调整
1. 在路线页面点「💬 聊天」按钮
2. 直接打字："孩子累了想休息" / "太晒了" / "想多看几个"
3. Agent 立即回复，并自动重新规划后半段
4. **杀手锏**：说"想去考拉馆"，Agent 帮你插队插入

### 5d. 闭馆过滤
1. 首页勾选「🕒 严格开闭馆」
2. 规划时跳过已闭馆的馆（如 16:30 后跳过犀鸟馆）

### 6. 合照彩蛋
1. 在路线页面点「📸 出片」按钮
2. 选择「📁 从相册选」或「📷 拍一张」
3. 上传后获得：徽章（"国宝认证"/"野菜F4认证"/"撞脸不撞DNA"等）+ 出片分 + 趣味评价 + 拍摄建议
4. 「📍 我在XX馆」可一键跳转打卡

### 7. 登录 + 个人历史
1. 顶部右侧「登录」按钮 → 注册（用户名+密码）
2. 登录后顶栏变成「👤 昵称」，点击可看个人面板：
   - 打卡次数 / 去过的馆 / 规划路线 / 照片评价 统计
   - 最近 5 条路线、打卡、照片
3. 数据存 SQLite（重启不丢），跨设备同步
4. 退出登录后再访问 → 数据隔离回匿名模式
5. **演示账号**：`demo` / `demo1234`（启动时自动创建）

### 8. 演示账号（已自动创建）
- 用户名：`demo`，密码：`demo1234`
- 启动脚本会自动创建（`start.sh` → `create_demo_user.py`）
- 数据持久化：重启 backend 不丢

## API 验证

```bash
# 健康检查
curl http://localhost:8000/api/health

# 文档
open http://localhost:8000/docs

# E2E 测试
cd /home/liyulingyue/Codes/CreativeProjects/ZooGuide
source Backend/.venv/bin/activate
python e2e_test.py
```

## 已知特性

- ✅ 23 个场馆（来自红山官方资料 + 百度百科）
- ✅ 规则引擎 + LLM 双路径，LLM 失败自动回退
- ✅ **步行矩阵**：haversine + 2.5x 路径倍数（红山是山地）
- ✅ 偏好个性化：时间/同行/体力/防晒/爬山/兴趣/入园门
- ✅ **严格开闭馆过滤**：可勾选（默认警告）
- ✅ 动态调整：基于反馈重新规划
- ✅ **聊天（自然语言调整）**：regex 快路径 + LLM 慢路径
- ✅ **路线对比**：必看/小众/平衡三种风格
- ✅ **SSE 流式输出**：可见 LLM 思考过程
- ✅ 动物打卡：localStorage + DB 双层
- ✅ **照片自动打卡**：识别匹配场馆自动 +1
- ✅ PWA：可"添加到主屏幕"，离线壳缓存
- ✅ **GPS 自动定位**：浏览器定位 → 最近场馆推荐 → 自动跳转
- ✅ **合照彩蛋**：拍照/选图 → 出片点评 + 红山专属徽章 + 拍摄建议
- ✅ **DB + 登录**：SQLite 存储，bcrypt 密码，UUID token，重启数据不丢

## 已知限制（合理范围内）

- LLM（glm-5）较慢：30-90s，但功能完整
- **glm-5 不支持图片识别**，合照评价走规则引擎兜底（红山梗库质量 OK）
- 步行时间 + 场馆坐标均为估算值（注明 "coord_note"），未实地测量
- 2025-10 新开放的南门新区信息基于公开资料
- 不接实时天气（让 LLM 给提示）
- 演示用，不做支付/购票

## 快速排查

| 问题 | 排查 |
|------|------|
| 前端 404 | 后端没起？`curl localhost:8000/api/health` |
| 路线规划卡死 | 看 /tmp/zooguide-backend.log，等 LLM 或勾选极速模式 |
| LLM 出错回退 | /tmp/zooguide-backend.log 应有 LLM error，fallback=true 仍可演示 |
| npm install 卡 | 用淘宝镜像：`npm config set registry https://registry.npmmirror.com` |
| 登录失败 | 检查 Backend/.env 里 OPENAI_* 配置；DB 在 Backend/data/zooguide.db |
| 重置数据 | `rm Backend/data/zooguide.db`，重启 backend 自动重建 |