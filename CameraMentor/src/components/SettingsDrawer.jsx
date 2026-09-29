import { useState } from 'react';
import { Icon } from './TopBar';

export default function SettingsDrawer({
  open,
  settings,
  onSave,
  onClose,
  onTest,
}) {
  const [draft, setDraft] = useState(settings);

  const set = (key, value) => setDraft((prev) => ({ ...prev, [key]: value }));

  return (
    <>
      <div
        className={`drawer-overlay ${open ? '' : 'hidden'}`}
        onClick={onClose}
      />
      <aside className={`drawer ${open ? '' : 'closed'}`}>
        <div className="drawer-header">
          <span className="drawer-title">设置</span>
          <button className="icon-btn" aria-label="关闭设置" onClick={onClose}>
            <Icon name="close" size={20} />
          </button>
        </div>

        <div className="drawer-body">
          <div className="field">
            <label>API 地址</label>
            <input
              type="url"
              value={draft.apiUrl}
              placeholder="https://api.example.com/analyze"
              autoComplete="off"
              onChange={(e) => set('apiUrl', e.target.value)}
            />
          </div>

          <div className="field">
            <label>请求格式</label>
            <div className="segmented">
              <button
                type="button"
                className={`seg-btn ${draft.requestFormat === 'multipart' ? 'active' : ''}`}
                onClick={() => set('requestFormat', 'multipart')}
              >
                multipart
              </button>
              <button
                type="button"
                className={`seg-btn ${draft.requestFormat === 'base64' ? 'active' : ''}`}
                onClick={() => set('requestFormat', 'base64')}
              >
                JSON base64
              </button>
            </div>
          </div>

          <div className="field">
            <label>图片字段名</label>
            <input
              type="text"
              value={draft.imageField}
              autoComplete="off"
              onChange={(e) => set('imageField', e.target.value || 'image')}
            />
          </div>

          <div className="field">
            <label>
              截帧频率 <span className="badge">{draft.frameRate.toFixed(1)} fps</span>
            </label>
            <input
              type="range"
              min="0.1"
              max="5"
              step="0.1"
              value={draft.frameRate}
              onChange={(e) => set('frameRate', parseFloat(e.target.value))}
            />
          </div>

          <div className="field">
            <label>摄像头</label>
            <div className="segmented">
              <button
                type="button"
                className={`seg-btn ${draft.facingMode === 'environment' ? 'active' : ''}`}
                onClick={() => set('facingMode', 'environment')}
              >
                后置
              </button>
              <button
                type="button"
                className={`seg-btn ${draft.facingMode === 'user' ? 'active' : ''}`}
                onClick={() => set('facingMode', 'user')}
              >
                前置
              </button>
            </div>
          </div>

          <div className="field">
            <label>Overlay 字段路径</label>
            <input
              type="text"
              value={draft.overlayField}
              placeholder="boxes / data.detections / …"
              autoComplete="off"
              onChange={(e) => set('overlayField', e.target.value)}
            />
            <small>响应 JSON 中检测框数组的路径，留空则不渲染 overlay</small>
          </div>

          <div className="field">
            <label>信息字段路径</label>
            <input
              type="text"
              value={draft.infoField}
              placeholder="analysis / text / …"
              autoComplete="off"
              onChange={(e) => set('infoField', e.target.value)}
            />
            <small>响应 JSON 中信息文本的路径，留空则显示完整 JSON</small>
          </div>

          <div className="field">
            <label className="switch-label">
              <span>Mock 模式</span>
              <label className="switch">
                <input
                  type="checkbox"
                  checked={draft.mockMode}
                  onChange={(e) => set('mockMode', e.target.checked)}
                />
                <span className="slider" />
              </label>
            </label>
            <small>无需 API，生成测试数据验证流程</small>
          </div>

          <div className="field">
            <label>额外请求头 (JSON)</label>
            <textarea
              rows={3}
              value={draft.extraHeaders}
              placeholder='{"Authorization":"Bearer xxx"}'
              onChange={(e) => set('extraHeaders', e.target.value || '{}')}
            />
          </div>

          <div className="drawer-actions">
            <button className="btn-secondary" onClick={() => onTest(draft)}>
              测试一次
            </button>
            <button className="btn-primary" onClick={() => onSave(draft)}>
              保存设置
            </button>
          </div>
        </div>
      </aside>
    </>
  );
}
