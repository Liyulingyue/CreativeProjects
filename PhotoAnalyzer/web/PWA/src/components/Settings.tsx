import type { AnalyzerConfig } from "../api/photoAnalyzer";

interface Props {
  config: AnalyzerConfig;
  onUpdate: (updates: Partial<AnalyzerConfig>) => void;
  onClearCache: () => void;
}

export function Settings({ config, onUpdate, onClearCache }: Props) {
  return (
    <>
      <div className="card">
        <div className="card-header">
          <div className="card-header-icon">🔑</div>
          <span>API 配置</span>
        </div>

        <div className="form-group">
          <label className="form-label">API Key</label>
          <input
            type="password"
            className="form-input"
            value={config.apiKey}
            onChange={(e) => onUpdate({ apiKey: e.target.value })}
            placeholder="请输入你的 API Key"
            autoComplete="off"
          />
        </div>

        <div className="form-group">
          <label className="form-label">Base URL</label>
          <input
            type="text"
            className="form-input"
            value={config.baseUrl}
            onChange={(e) => onUpdate({ baseUrl: e.target.value })}
          />
        </div>

        <div className="form-row">
          <div className="form-group">
            <label className="form-label">模型</label>
            <input
              type="text"
              className="form-input"
              value={config.model}
              onChange={(e) => onUpdate({ model: e.target.value })}
            />
          </div>
          <div className="form-group">
            <label className="form-label">请求间隔 (ms)</label>
            <input
              type="number"
              className="form-input"
              value={config.delay}
              min={0}
              onChange={(e) =>
                onUpdate({ delay: parseInt(e.target.value) || 0 })
              }
            />
          </div>
        </div>
      </div>

      <div className="card">
        <div className="card-header">
          <div className="card-header-icon">💾</div>
          <span>数据管理</span>
        </div>
        <button
          className="btn btn-secondary"
          onClick={() => {
            if (confirm("确定要清空所有本地数据吗？\n（包括图片、分析结果和设置）")) {
              onClearCache();
            }
          }}
        >
          🗑️ 清空所有本地数据
        </button>
      </div>

      <div className="card">
        <div className="card-header">
          <div className="card-header-icon">ℹ️</div>
          <span>关于</span>
        </div>
        <div className="about-text">
          <div className="about-row">
            <span>版本</span>
            <span>v1.0.0</span>
          </div>
          <div className="about-row">
            <span>技术栈</span>
            <span>React + Vite + PWA</span>
          </div>
          <div className="about-row">
            <span>说明</span>
            <span>所有数据本地处理</span>
          </div>
        </div>
      </div>
    </>
  );
}