import { useState, useEffect, useCallback } from "react";
import {
  getCacheStats,
  getCacheEntries,
  clearCache,
  deleteCacheEntry,
  exportCacheToFolder,
  importCacheFromFolder,
} from "@/api/dedup";
import { getSettings } from "@/api/settings";
import type { CacheStats, CacheEntry, AppSettings } from "@/api/types";

const TYPE_LABELS: Record<string, string> = {
  hash_phash: "pHash 哈希",
  hash_ahash: "aHash 哈希",
  hash_dhash: "dHash 哈希",
  hash_multihash: "多哈希",
  emb_clip: "CLIP 嵌入",
  emb_resnet50: "ResNet50 嵌入",
  emb_resnet18: "ResNet18 嵌入",
  exif: "EXIF 信息",
};

const TYPE_DESC: Record<string, string> = {
  hash_phash: "感知哈希，用于检测视觉相似图片",
  hash_ahash: "均值哈希，最简单的相似度检测",
  hash_dhash: "梯度哈希，检测结构相似性",
  hash_multihash: "多种哈希综合判定",
  emb_clip: "CLIP 模型 512 维特征向量",
  emb_resnet50: "ResNet50 模型 2048 维特征向量",
  emb_resnet18: "ResNet18 模型 512 维特征向量",
  exif: "拍摄时间、相机型号等元数据",
};

export function Cache() {
  const [stats, setStats] = useState<CacheStats>({});
  const [entries, setEntries] = useState<CacheEntry[]>([]);
  const [selectedType, setSelectedType] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [clearing, setClearing] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [converting, setConverting] = useState(false);

  const loadSettings = useCallback(async () => {
    try {
      const s = await getSettings();
      setSettings(s);
    } catch {}
  }, []);

  const loadStats = useCallback(async () => {
    try {
      const s = await getCacheStats();
      setStats(s);
    } catch {}
  }, []);

  const loadEntries = useCallback(async () => {
    setLoading(true);
    try {
      const e = await getCacheEntries(selectedType || undefined);
      setEntries(e);
    } catch {
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }, [selectedType]);

  useEffect(() => {
    loadSettings();
    loadStats();
  }, [loadSettings, loadStats]);

  const legacyFolderCount = Number(stats.folder || 0);
  const normalizedStats = Object.fromEntries(
    Object.entries(stats).filter(([k]) => k !== "folder")
  ) as CacheStats;
  const totalCount = Object.values(normalizedStats).reduce((a, b) => a + b, 0);
  const types = Object.keys(normalizedStats);
  const storageModeText = settings?.storage_mode === "project" ? "项目模式" : settings?.storage_mode === "folder" ? "文件夹模式" : "未读取";
  const migrationHint = settings?.storage_mode === "project"
    ? "当前统计的是项目缓存。可手动导出到各图片目录的 .photoanalyzer/，用于迁移。"
    : "当前统计的是文件夹缓存。可手动导入到项目缓存，用于迁移。";

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  useEffect(() => {
    if (selectedType && !Object.prototype.hasOwnProperty.call(normalizedStats, selectedType)) {
      setSelectedType(null);
    }
  }, [selectedType, normalizedStats]);

  const handleClearAll = async () => {
    if (!confirm("确定要清除所有缓存吗？此操作不可恢复。")) return;
    setClearing(true);
    try {
      await clearCache();
      await loadStats();
      await loadEntries();
    } finally {
      setClearing(false);
    }
  };

  const handleClearType = async (type: string) => {
    if (!confirm(`确定要清除所有 ${TYPE_LABELS[type] || type} 缓存吗？`)) return;
    setClearing(true);
    try {
      await clearCache(type);
      await loadStats();
      await loadEntries();
    } finally {
      setClearing(false);
    }
  };

  const handleDeleteEntry = async (key: string) => {
    try {
      await deleteCacheEntry(key);
      await loadStats();
      await loadEntries();
    } catch {}
  };

  return (
    <div className="page">
      <h1>缓存管理</h1>

      <div className="card">
        <div className="cache-overview">
          <div className="cache-overview__stat">
            <span className="cache-overview__value">{totalCount}</span>
            <span className="cache-overview__label">总缓存条目</span>
          </div>
          <div className="cache-overview__stat">
            <span className="cache-overview__value">{types.length}</span>
            <span className="cache-overview__label">特征类型</span>
          </div>
          <div className="cache-overview__stat">
            <span className="cache-overview__value">{storageModeText}</span>
            <span className="cache-overview__label">当前存储模式</span>
          </div>
          <div className="cache-overview__actions">
            {settings && (
              <>
                {settings.storage_mode === "project" ? (
                  <button
                    className="btn"
                    onClick={async () => {
                      if (!confirm("将项目缓存导出到各图片目录的 .photoanalyzer/ 文件夹？")) return;
                      setConverting(true);
                      try {
                        const r = await exportCacheToFolder();
                        alert(`已导出 ${r.migrated} 条缓存到 ${r.directories} 个目录`);
                        await loadStats();
                        await loadEntries();
                      } catch (e) {
                        alert(e instanceof Error ? e.message : "导出失败");
                      } finally {
                        setConverting(false);
                      }
                    }}
                    disabled={converting || totalCount === 0}
                  >
                    迁移到文件夹缓存
                  </button>
                ) : (
                  <button
                    className="btn"
                    onClick={async () => {
                      if (!confirm("将各 .photoanalyzer/ 缓存导入到项目缓存？")) return;
                      setConverting(true);
                      try {
                        const r = await importCacheFromFolder();
                        alert(`已导入 ${r.migrated} 条缓存`);
                        await loadStats();
                        await loadEntries();
                      } catch (e) {
                        alert(e instanceof Error ? e.message : "导入失败");
                      } finally {
                        setConverting(false);
                      }
                    }}
                    disabled={converting}
                  >
                    迁移到项目缓存
                  </button>
                )}
              </>
            )}
            <button
              className="btn btn--danger"
              onClick={handleClearAll}
              disabled={clearing || totalCount === 0}
            >
              {clearing ? "清除中..." : "清除全部缓存"}
            </button>
          </div>
        </div>
        <div className="cache-overview__hint">
          {migrationHint}
        </div>
        {legacyFolderCount > 0 && (
          <div className="cache-overview__hint">
            检测到旧版统计字段 folder={legacyFolderCount}。该字段表示目录容器数量，不是可筛选的特征类型，已在页面中自动忽略。
          </div>
        )}
      </div>

      <div className="card">
        <h3>按类型查看</h3>
        <div className="cache-type-grid">
          {types.length === 0 && (
            <div className="empty-hint">暂无缓存数据</div>
          )}
          {types.map((type) => (
            <div
              key={type}
              className={`cache-type-card ${selectedType === type ? "cache-type-card--active" : ""}`}
              onClick={() => setSelectedType(selectedType === type ? null : type)}
            >
              <div className="cache-type-card__header">
                <span className="cache-type-card__name">
                  {TYPE_LABELS[type] || type}
                </span>
                <span className="cache-type-card__count">{normalizedStats[type]}</span>
              </div>
              <div className="cache-type-card__desc">
                {TYPE_DESC[type] || type}
              </div>
              <button
                className="btn btn--sm btn--danger"
                onClick={(e) => {
                  e.stopPropagation();
                  handleClearType(type);
                }}
                disabled={clearing}
              >
                清除
              </button>
            </div>
          ))}
        </div>
      </div>

      <div className="card">
        <div className="cache-entries-header">
          <h3>
            缓存条目
            {selectedType ? ` · ${TYPE_LABELS[selectedType] || selectedType}` : ""}
            <span className="cache-entries-count">{entries.length}</span>
          </h3>
        </div>

        {loading ? (
          <div className="loading">加载中...</div>
        ) : entries.length === 0 ? (
          <div className="empty-hint">暂无缓存条目</div>
        ) : (
          <div className="cache-entry-list">
            {entries.map((entry, idx) => (
              <div key={`${entry.feature_type}:${entry.cache_key}:${entry.file_path}:${entry.mtime}:${idx}`} className="cache-entry">
                <div className="cache-entry__info">
                  <div className="cache-entry__path" title={entry.file_path}>
                    {entry.file_path}
                  </div>
                  <div className="cache-entry__meta">
                    <span className="cache-entry__type">
                      {TYPE_LABELS[entry.feature_type] || entry.feature_type}
                    </span>
                    <span className="cache-entry__mtime">
                      {new Date(entry.mtime * 1000).toLocaleString()}
                    </span>
                  </div>
                  {entry.data && (
                    <div className="cache-entry__data">
                      {Object.entries(entry.data).map(([k, v]) => (
                        <span key={k} className="cache-entry__datum">
                          {k}: {typeof v === "object" ? JSON.stringify(v) : String(v)}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
                <button
                  className="btn btn--sm btn--danger"
                  onClick={() => handleDeleteEntry(entry.cache_key)}
                  title="删除此条目"
                >
                  删除
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
