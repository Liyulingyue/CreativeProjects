import { useState, useEffect } from "react";
import { getSettings, updateSettings } from "@/api/settings";
import type { AppSettings, DedupStageConfig } from "@/api/types";

const DEFAULT_SETTINGS: AppSettings = {
  api_key: "",
  base_url: "https://api.minimaxi.com/v1",
  model: "MiniMax-M3",
  delay: 1000,
  storage_mode: "project",
  dedup_stages: [
    { type: "exif", enabled: true, params: { time_window: 5 } },
    { type: "phash", enabled: true, params: { threshold: 8 } },
    { type: "embedding", enabled: false, params: { model: "clip", threshold: 0.9 } },
  ],
};

export function Settings() {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showApiKey, setShowApiKey] = useState(false);

  useEffect(() => {
    getSettings()
      .then((s) => setSettings({ ...DEFAULT_SETTINGS, ...s }))
      .catch(() => {});
  }, []);

  const handleChange = (key: keyof AppSettings, value: unknown) => {
    setSettings((prev) => ({ ...prev, [key]: value }));
    setSaved(false);
  };

  const handleSave = async () => {
    try {
      const result = await updateSettings(settings);
      setSettings(result);
      setSaved(true);
      setError(null);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save");
    }
  };

  const toggleStage = (index: number) => {
    setSettings((prev) => {
      const stages = [...prev.dedup_stages];
      stages[index] = { ...stages[index], enabled: !stages[index].enabled };
      return { ...prev, dedup_stages: stages };
    });
    setSaved(false);
  };

  return (
    <div className="page">
      <h1>设置</h1>

      <div className="card">
        <h3>API 配置</h3>

        <div className="form-group">
          <label>API Key</label>
          <div className="input-with-action">
            <input
              type={showApiKey ? "text" : "password"}
              value={settings.api_key}
              onChange={(e) => handleChange("api_key", e.target.value)}
              placeholder="sk-..."
            />
            <button
              className="btn btn--sm"
              onClick={() => setShowApiKey(!showApiKey)}
            >
              {showApiKey ? "隐藏" : "显示"}
            </button>
          </div>
        </div>

        <div className="form-group">
          <label>Base URL</label>
          <input
            type="text"
            value={settings.base_url}
            onChange={(e) => handleChange("base_url", e.target.value)}
          />
        </div>

        <div className="form-group">
          <label>模型</label>
          <input
            type="text"
            value={settings.model}
            onChange={(e) => handleChange("model", e.target.value)}
          />
        </div>

        <div className="form-group">
          <label>请求间隔 (ms)</label>
          <input
            type="number"
            value={settings.delay}
            onChange={(e) => handleChange("delay", Number(e.target.value))}
            min={0}
            step={100}
          />
        </div>
      </div>

      <div className="card">
        <h3>缓存存储模式</h3>
        <div className="storage-mode">
          <div
            className={`storage-mode__option ${settings.storage_mode === "project" ? "storage-mode__option--active" : ""}`}
            onClick={() => handleChange("storage_mode", "project")}
          >
            <div className="storage-mode__header">
              <span className="storage-mode__name">项目模式</span>
              {settings.storage_mode === "project" && <span className="storage-mode__badge">当前</span>}
            </div>
            <div className="storage-mode__desc">
              缓存数据集中存储在 PhotoAnalyzer/data/ 目录下，与程序绑定。换机器需重新计算。
            </div>
          </div>
          <div
            className={`storage-mode__option ${settings.storage_mode === "folder" ? "storage-mode__option--active" : ""}`}
            onClick={() => handleChange("storage_mode", "folder")}
          >
            <div className="storage-mode__header">
              <span className="storage-mode__name">文件夹模式</span>
              {settings.storage_mode === "folder" && <span className="storage-mode__badge">当前</span>}
            </div>
            <div className="storage-mode__desc">
              缓存数据存储在图片目录下的 .photoanalyzer/ 隐藏文件夹中，与图片绑定。移动文件夹后缓存仍有效。
            </div>
          </div>
        </div>
      </div>

      <div className="card">
        <h3>去重阶段配置</h3>
        {settings.dedup_stages.map((stage: DedupStageConfig, i: number) => (
          <div key={stage.type} className="stage-config">
            <label className="stage-config__label">
              <input
                type="checkbox"
                checked={stage.enabled}
                onChange={() => toggleStage(i)}
              />
              <span>{stage.type === "exif" ? "EXIF 时间分组" : stage.type === "phash" ? "感知哈希去重" : "嵌入特征去重"}</span>
            </label>
            <span className="stage-config__desc">
              {stage.type === "exif"
                ? "按拍摄时间和相机型号分组"
                : stage.type === "phash"
                  ? "基于感知哈希检测相似图片"
                  : "基于 CLIP/ResNet 深度特征聚类"}
            </span>
          </div>
        ))}
      </div>

      <div className="settings-actions">
        <button className="btn btn--primary" onClick={handleSave}>
          {saved ? "已保存 ✓" : "保存设置"}
        </button>
        {error && <span className="error-msg">{error}</span>}
      </div>
    </div>
  );
}
