import { useState, useEffect } from 'react';

interface Props {
  onClose: () => void;
}

export function AdminSettingsModal({ onClose }: Props) {
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState<{ text: string; err?: boolean } | null>(null);

  const [refreshEnabled, setRefreshEnabled] = useState(false);
  const [refreshMode, setRefreshMode] = useState<'interval' | 'daily'>('interval');
  const [refreshInterval, setRefreshInterval] = useState(3600);
  const [dailyHour, setDailyHour] = useState(3);
  const [maxOffset, setMaxOffset] = useState(1);
  const [maxDuration, setMaxDuration] = useState(2);
  const [concurrency, setConcurrency] = useState(3);
  const [apiKey, setApiKey] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [modelName, setModelName] = useState('');

  useEffect(() => {
    fetch('/api/admin/config', {
      headers: { Authorization: `Bearer ${localStorage.getItem('travelsites_token')}` },
    })
      .then((r) => r.json())
      .then((d) => {
        setRefreshEnabled(d.refresh_enabled ?? false);
        setRefreshMode(d.refresh_mode ?? 'interval');
        setRefreshInterval(d.refresh_interval_seconds ?? 3600);
        setDailyHour(d.daily_run_hour ?? 3);
        setMaxOffset(d.matrix_max_offset ?? 1);
        setMaxDuration(d.matrix_max_duration ?? 2);
        setConcurrency(d.matrix_concurrency ?? 3);
        setApiKey(d.api_key ?? '');
        setBaseUrl(d.base_url ?? '');
        setModelName(d.model_name ?? '');
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  const handleSave = async () => {
    setSaving(true);
    setMsg(null);
    try {
      const updates: any = {
        refresh_enabled: refreshEnabled,
        refresh_mode: refreshMode,
        refresh_interval_seconds: refreshInterval,
        daily_run_hour: dailyHour,
        matrix_max_offset: maxOffset,
        matrix_max_duration: maxDuration,
        matrix_concurrency: concurrency,
        model_name: modelName,
      };
      if (apiKey && !apiKey.includes('****')) updates.api_key = apiKey;
      if (baseUrl) updates.base_url = baseUrl;

      const res = await fetch('/api/admin/config', {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${localStorage.getItem('travelsites_token')}`,
        },
        body: JSON.stringify(updates),
      });
      const data = await res.json();
      if (res.ok) {
        setMsg({ text: '✓ 已保存' });
        if (data.config?.api_key) setApiKey(data.config.api_key);
        setTimeout(() => onClose(), 800);
      } else {
        setMsg({ text: data.detail || '保存失败', err: true });
      }
    } catch {
      setMsg({ text: '保存失败', err: true });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>系统设置</h2>
          <button className="modal-close" onClick={onClose}>×</button>
        </div>

        <div className="modal-body">
          {loading ? (
            <p className="settings-loading">加载中…</p>
          ) : (
            <>
              <div className="setting-block">
                <div className="setting-block-title">
                  <h3>定时刷新</h3>
                  <p>定期自动更新所有城市的数据</p>
                </div>
                <div className="setting-block-fields">
                  <label className="switch-row">
                    <span>启用</span>
                    <input
                      type="checkbox"
                      className="switch-input"
                      checked={refreshEnabled}
                      onChange={(e) => setRefreshEnabled(e.target.checked)}
                    />
                  </label>
                  {refreshEnabled && (
                    <>
                      <div className="setting-row">
                        <span>模式</span>
                        <div className="seg-group">
                          <button
                            type="button"
                            className={`seg-btn ${refreshMode === 'interval' ? 'active' : ''}`}
                            onClick={() => setRefreshMode('interval')}
                          >间隔</button>
                          <button
                            type="button"
                            className={`seg-btn ${refreshMode === 'daily' ? 'active' : ''}`}
                            onClick={() => setRefreshMode('daily')}
                          >每日定时</button>
                        </div>
                      </div>
                      {refreshMode === 'interval' ? (
                        <div className="setting-row">
                          <span>间隔</span>
                          <div className="seg-group">
                            {[
                              { label: '1h', value: 3600 },
                              { label: '6h', value: 21600 },
                              { label: '12h', value: 43200 },
                              { label: '24h', value: 86400 },
                            ].map((o) => (
                              <button
                                key={o.value}
                                type="button"
                                className={`seg-btn ${refreshInterval === o.value ? 'active' : ''}`}
                                onClick={() => setRefreshInterval(o.value)}
                              >
                                {o.label}
                              </button>
                            ))}
                          </div>
                        </div>
                      ) : (
                        <div className="setting-row">
                          <span>每天</span>
                          <div className="stepper">
                            <button onClick={() => setDailyHour((dailyHour + 23) % 24)}>−</button>
                            <span>{String(dailyHour).padStart(2, '0')}:00</span>
                            <button onClick={() => setDailyHour((dailyHour + 1) % 24)}>+</button>
                          </div>
                        </div>
                      )}
                    </>
                  )}
                </div>
              </div>

              <div className="setting-block">
                <div className="setting-block-title">
                  <h3>矩阵生成</h3>
                  <p>每个城市缓存的行程方案网格</p>
                </div>
                <div className="setting-block-fields">
                  <div className="setting-row">
                    <span>最大偏移</span>
                    <div className="stepper">
                      <button onClick={() => setMaxOffset(Math.max(1, maxOffset - 1))}>−</button>
                      <span>{maxOffset} 天</span>
                      <button onClick={() => setMaxOffset(Math.min(30, maxOffset + 1))}>+</button>
                    </div>
                  </div>
                  <div className="setting-row">
                    <span>最大天数</span>
                    <div className="stepper">
                      <button onClick={() => setMaxDuration(Math.max(1, maxDuration - 1))}>−</button>
                      <span>{maxDuration} 天</span>
                      <button onClick={() => setMaxDuration(Math.min(14, maxDuration + 1))}>+</button>
                    </div>
                  </div>
                  <div className="setting-row">
                    <span>并发数</span>
                    <div className="stepper">
                      <button onClick={() => setConcurrency(Math.max(1, concurrency - 1))}>−</button>
                      <span>{concurrency}</span>
                      <button onClick={() => setConcurrency(Math.min(10, concurrency + 1))}>+</button>
                    </div>
                  </div>
                </div>
              </div>

              <div className="setting-block">
                <div className="setting-block-title">
                  <h3>AI 配置</h3>
                  <p>行程规划使用的模型</p>
                </div>
                <div className="setting-block-fields">
                  <div className="setting-row stacked">
                    <span>API Key</span>
                    <input
                      type="password"
                      className="text-input"
                      placeholder={apiKey || '输入新值则更新'}
                      value={apiKey}
                      onChange={(e) => setApiKey(e.target.value)}
                    />
                  </div>
                  <div className="setting-row stacked">
                    <span>Base URL</span>
                    <input
                      type="text"
                      className="text-input"
                      placeholder="https://api.minimaxi.com/v1"
                      value={baseUrl}
                      onChange={(e) => setBaseUrl(e.target.value)}
                    />
                  </div>
                  <div className="setting-row stacked">
                    <span>模型</span>
                    <input
                      type="text"
                      className="text-input"
                      placeholder="MiniMax-M3"
                      value={modelName}
                      onChange={(e) => setModelName(e.target.value)}
                    />
                  </div>
                </div>
              </div>

              {msg && (
                <p className="settings-msg" style={{ color: msg.err ? 'var(--danger)' : 'var(--success)' }}>
                  {msg.text}
                </p>
              )}
            </>
          )}
        </div>

        <div className="modal-footer">
          <button className="btn btn-secondary" onClick={onClose}>取消</button>
          <button className="btn btn-primary" onClick={handleSave} disabled={saving || loading}>
            {saving ? '保存中…' : '保存'}
          </button>
        </div>
      </div>
    </div>
  );
}
