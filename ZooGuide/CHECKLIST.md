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
- ✅ 步行时间矩阵估算（基于官方距离数据）
- ✅ 偏好个性化：时间/同行/体力/防晒/爬山/兴趣/入园门
- ✅ 动态调整：基于反馈重新规划
- ✅ 动物打卡：localStorage 持久化
- ✅ PWA：可"添加到主屏幕"，离线壳缓存
- ✅ CORS：前端 Vite proxy 已配

## 已知限制（合理范围内）

- LLM（glm-5）较慢：30-90s，但功能完整
- 步行时间为估算值，未实地测量
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