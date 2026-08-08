import { useState, useEffect } from "react";
import { AppConfig } from "../types";

interface SettingsProps {
  config: AppConfig | null;
  onSave: (config: AppConfig) => Promise<void>;
  onClose: () => void;
}

function fmtSec(secs: number): string {
  if (secs < 60) return `${secs}秒`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return s === 0 ? `${m}分` : `${m}分${s}秒`;
}

export default function Settings({ config, onSave, onClose }: SettingsProps) {
  const [local, setLocal] = useState<AppConfig>(config ?? defaultConfig());
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (config) setLocal(config);
  }, [config]);

  const handleSave = async () => {
    if (local.away_threshold_seconds < local.check_interval_seconds * 2) {
      setLocal((prev) => ({ ...prev, away_threshold_seconds: prev.check_interval_seconds * 3 }));
    }
    setSaving(true);
    await onSave(local);
    setSaving(false);
    onClose();
  };

  const update = <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => {
    setLocal((prev) => ({ ...prev, [key]: value }));
  };

  return (
    <div className="settings-panel">
      <h2>设置</h2>

      <div className="setting-group">
        <h3>工作提醒</h3>
        <div className="setting-item">
          <div>
            <label>工作时长阈值</label>
            <div className="description">连续工作多久后提醒休息</div>
          </div>
          <div className="setting-control">
            <input
              type="range"
              min={10}
              max={120}
              step={5}
              value={local.work_threshold_minutes}
              onChange={(e) => update("work_threshold_minutes", Number(e.target.value))}
            />
            <span className="range-value">{local.work_threshold_minutes}分</span>
          </div>
        </div>
        <div className="setting-item">
          <div>
            <label>重复提醒间隔</label>
            <div className="description">超时后每隔多久再次提醒</div>
          </div>
          <div className="setting-control">
            <input
              type="range"
              min={1}
              max={30}
              step={1}
              value={local.remind_interval_minutes}
              onChange={(e) => update("remind_interval_minutes", Number(e.target.value))}
            />
            <span className="range-value">{local.remind_interval_minutes}分</span>
          </div>
        </div>
      </div>

      <div className="setting-group">
        <h3>摄像头</h3>
        <div className="setting-item">
          <label>画面镜像</label>
          <button
            className={`toggle ${local.mirror_video ? "active" : ""}`}
            onClick={() => update("mirror_video", !local.mirror_video)}
          />
        </div>
      </div>

      <div className="setting-group">
        <h3>坐姿检测</h3>
        <div className="setting-item">
          <div>
            <label>坐姿提醒阈值</label>
            <div className="description">评分低于此值时提醒调整坐姿</div>
          </div>
          <div className="setting-control">
            <input
              type="range"
              min={20}
              max={80}
              step={5}
              value={local.posture_alert_threshold}
              onChange={(e) => update("posture_alert_threshold", Number(e.target.value))}
            />
            <span className="range-value">{local.posture_alert_threshold}</span>
          </div>
        </div>
      </div>

      <div className="setting-group">
        <h3>检测</h3>
        <div className="setting-item">
          <div>
            <label>检测间隔</label>
            <div className="description">多久检测一次人在否及坐姿</div>
          </div>
          <div className="setting-control">
            <input
              type="number"
              min={1}
              max={300}
              value={local.check_interval_seconds}
              onChange={(e) => {
                const v = Math.max(1, Math.min(300, Number(e.target.value) || 1));
                update("check_interval_seconds", v);
              }}
            />
            <span className="range-value">{fmtSec(local.check_interval_seconds)}</span>
          </div>
        </div>
        <div className="setting-item">
          <div>
            <label>离开判定</label>
            <div className="description">连续未检测到人多久后判定为离开</div>
          </div>
          <div className="setting-control">
            <input
              type="number"
              min={local.check_interval_seconds * 2}
              max={600}
              value={local.away_threshold_seconds}
              onChange={(e) => {
                const v = Math.max(local.check_interval_seconds * 2, Math.min(600, Number(e.target.value) || 1));
                update("away_threshold_seconds", v);
              }}
            />
            <span className="range-value">{fmtSec(local.away_threshold_seconds)}</span>
          </div>
        </div>
      </div>

      <div className="setting-group">
        <h3>通用</h3>
        <div className="setting-item">
          <label>启动后自动开始监控</label>
          <button
            className={`toggle ${local.auto_start_monitoring ? "active" : ""}`}
            onClick={() => update("auto_start_monitoring", !local.auto_start_monitoring)}
          />
        </div>
        <div className="setting-item">
          <label>提醒声音</label>
          <button
            className={`toggle ${local.notification_sound ? "active" : ""}`}
            onClick={() => update("notification_sound", !local.notification_sound)}
          />
        </div>
      </div>

      <div className="settings-actions">
        <button className="btn btn-secondary" onClick={onClose}>取消</button>
        <button className="btn btn-primary" onClick={handleSave} disabled={saving}>
          {saving ? "保存中..." : "保存"}
        </button>
      </div>
    </div>
  );
}

function defaultConfig(): AppConfig {
  return {
    work_threshold_minutes: 20,
    remind_interval_minutes: 5,
    away_threshold_seconds: 90,
    check_interval_seconds: 30,
    posture_alert_threshold: 50,
    auto_start_monitoring: true,
    notification_sound: true,
    mirror_video: true,
  };
}
