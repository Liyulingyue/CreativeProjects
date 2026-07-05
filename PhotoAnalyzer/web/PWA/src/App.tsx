import { useState, useEffect } from "react";
import {
  analyzeImages,
  exportToJson,
  exportToCsv,
  type AnalyzerConfig,
} from "./api/photoAnalyzer";
import {
  saveRecords,
  loadRecords,
  clearAllData,
  clearAnalyzedRecords,
  deleteRecord,
  type RecordEntry,
} from "./api/storage";
import { BottomNav } from "./components/BottomNav";
import { Gallery } from "./components/Gallery";
import { ResultsList } from "./components/ResultsList";
import { ImageDetail } from "./components/ImageDetail";
import { Settings } from "./components/Settings";
import type { TabType, AnalysisLog } from "./types";

const DEFAULT_CONFIG: AnalyzerConfig = {
  apiKey: "",
  baseUrl: "https://api.minimaxi.com/v1",
  model: "MiniMax-M3",
  delay: 1000,
  maxCacheCount: 10,
};

export default function App() {
  const [config, setConfig] = useState<AnalyzerConfig>(() => {
    const saved = localStorage.getItem("photo-analyzer-config");
    return saved ? { ...DEFAULT_CONFIG, ...JSON.parse(saved) } : DEFAULT_CONFIG;
  });
  const [records, setRecords] = useState<RecordEntry[]>([]);
  const [progress, setProgress] = useState({ current: 0, total: 0 });
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [analysisLog, setAnalysisLog] = useState<AnalysisLog[]>([]);
  const [activeTab, setActiveTab] = useState<TabType>("images");
  const [detailIndex, setDetailIndex] = useState<number | null>(null);
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    return (localStorage.getItem("theme") as "light" | "dark") || "light";
  });
  const [toast, setToast] = useState<string>("");
  const [online, setOnline] = useState(navigator.onLine);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("theme", theme);
  }, [theme]);

  useEffect(() => {
    const handleOnline = () => setOnline(true);
    const handleOffline = () => setOnline(false);
    window.addEventListener("online", handleOnline);
    window.addEventListener("offline", handleOffline);
    return () => {
      window.removeEventListener("online", handleOnline);
      window.removeEventListener("offline", handleOffline);
    };
  }, []);

  useEffect(() => {
    const loadCachedData = async () => {
      try {
        const cached = await loadRecords(config.maxCacheCount);
        if (cached.length > 0) {
          setRecords(cached);
        }
      } catch (e) {
        console.warn("Failed to load cached data:", e);
      }
    };
    loadCachedData();
  }, []);

  const showToast = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(""), 2400);
  };

  const updateConfig = (updates: Partial<AnalyzerConfig>) => {
    const newConfig = { ...config, ...updates };
    setConfig(newConfig);
    localStorage.setItem("photo-analyzer-config", JSON.stringify(newConfig));
  };

  const addFiles = async (files: File[]) => {
    const newRecords = await Promise.all(
      files.map(async (file) => {
        const buffer = await file.arrayBuffer();
        const dataUrl = await new Promise<string>((resolve) => {
          const reader = new FileReader();
          reader.onload = (e) => resolve(e.target?.result as string);
          reader.readAsDataURL(file);
        });
        return {
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
          fileName: file.name,
          fileType: file.type,
          data: buffer,
          thumb: dataUrl,
          addedAt: Date.now(),
          result: null,
          analyzedAt: null,
          failedAt: null,
        };
      })
    );

    setRecords((prev) => {
      const updated = [...prev, ...newRecords];
      saveRecords(newRecords, config.maxCacheCount).catch(console.warn);
      return updated;
    });
    showToast(`已添加 ${files.length} 张图片`);
  };

  const removeFile = (id: string) => {
    setRecords((prev) => prev.filter((r) => r.id !== id));
    deleteRecord(id).catch(console.warn);
  };

  const clearFiles = () => {
    setRecords((prev) => {
      const remaining = prev.filter(
        (r) => r.analyzedAt || r.failedAt
      );
      const removed = prev.filter((r) => !r.analyzedAt && !r.failedAt);
      removed.forEach((r) => deleteRecord(r.id).catch(console.warn));
      return remaining;
    });
    showToast("已清空待分析");
  };

  const clearResults = () => {
    if (!confirm("确定要清空所有分析历史吗？")) return;
    setRecords((prev) => prev.filter((r) => !r.analyzedAt));
    clearAnalyzedRecords().catch(console.warn);
    showToast("已清空分析历史");
  };

  const handleAnalyze = async () => {
    if (!config.apiKey) {
      showToast("请先在「关于」中填写 API Key");
      setActiveTab("settings");
      return;
    }

    const pending = records.filter((r) => !r.analyzedAt);
    if (pending.length === 0) {
      showToast("请先添加图片");
      return;
    }

    setIsAnalyzing(true);
    setProgress({ current: 0, total: pending.length });
    setAnalysisLog([]);

    let successCount = 0;
    const log: AnalysisLog[] = [];

    for (let i = 0; i < pending.length; i++) {
      const record = pending[i];
      const blob = new Blob([record.data], { type: record.fileType });
      const file = new File([blob], record.fileName, { type: record.fileType });

      const result = await analyzeImages([file], config);
      const r = result[0];

      setProgress({ current: i + 1, total: pending.length });

      if (r.success) {
        successCount++;
        log.push({
          fileName: record.fileName,
          status: "success",
          score: r.data?.score,
        });
      } else {
        log.push({
          fileName: record.fileName,
          status: "failed",
          error: r.error || "未知错误",
        });
        showToast(`❌ ${record.fileName}: ${r.error || "分析失败"}`);
      }

      const updated = {
        ...record,
        result: r,
        analyzedAt: r.success ? Date.now() : null,
        failedAt: r.success ? null : Date.now(),
      };

      setAnalysisLog([...log]);

      setRecords((prev) => {
        const next = prev.map((rec) => (rec.id === record.id ? updated : rec));
        const analyzed = next.filter((r) => r.analyzedAt);
        if (analyzed.length > config.maxCacheCount) {
          const sorted = analyzed.sort(
            (a, b) => (a.analyzedAt || 0) - (b.analyzedAt || 0)
          );
          const toRemove = sorted
            .slice(0, analyzed.length - config.maxCacheCount)
            .map((r) => r.id);
          return next.filter((r) => !toRemove.includes(r.id));
        }
        return next;
      });
      saveRecords([updated], config.maxCacheCount).catch(console.warn);

      if (i < pending.length - 1 && config.delay > 0) {
        await new Promise((res) => setTimeout(res, config.delay));
      }
    }

    setIsAnalyzing(false);
    showToast(`分析完成！${successCount}/${pending.length} 成功`);
    setActiveTab("results");
  };

  const handleExportJson = () => {
    const analyzed = records.filter((r) => r.result);
    if (analyzed.length === 0) return;
    const results = analyzed.map((r) => r.result!);
    exportToJson(results);
    showToast("已导出 JSON");
  };

  const handleExportCsv = () => {
    const analyzed = records.filter((r) => r.result);
    if (analyzed.length === 0) return;
    const results = analyzed.map((r) => r.result!);
    exportToCsv(results);
    showToast("已导出 CSV");
  };

  const handleClearAll = async () => {
    localStorage.clear();
    await clearAllData();
    setConfig(DEFAULT_CONFIG);
    setRecords([]);
    setProgress({ current: 0, total: 0 });
    setTheme("light");
    document.documentElement.setAttribute("data-theme", "light");
    showToast("已清空所有数据");
  };

  const pendingFiles = records.filter((r) => !r.analyzedAt);
  const analyzedRecords = records.filter((r) => r.analyzedAt && r.result);

  return (
    <div className="app">
      <header className="app-header">
        <div className="app-logo">
          <div className="app-logo-icon">📷</div>
          <span>PhotoAnalyzer</span>
        </div>
        <button
          className="theme-toggle"
          onClick={() => setTheme(theme === "light" ? "dark" : "light")}
          aria-label="切换主题"
        >
          {theme === "light" ? "🌙" : "☀️"}
        </button>
      </header>

      {!online && (
        <div className="offline-banner">📴 当前离线，分析功能不可用</div>
      )}

      <main className="app-content">
        {activeTab === "images" && (
          <Gallery
            records={pendingFiles}
            onAdd={addFiles}
            onRemove={removeFile}
            onClear={clearFiles}
            onAnalyze={handleAnalyze}
            isAnalyzing={isAnalyzing}
            progress={progress}
            log={analysisLog}
            disabled={isAnalyzing}
          />
        )}

        {activeTab === "results" && (
          <ResultsList
            records={analyzedRecords}
            onSelect={(i) => setDetailIndex(i)}
            onExportJson={handleExportJson}
            onExportCsv={handleExportCsv}
            onClear={clearResults}
          />
        )}

        {activeTab === "settings" && (
          <Settings
            config={config}
            onUpdate={updateConfig}
            onClearCache={handleClearAll}
          />
        )}
      </main>

      <BottomNav
        active={activeTab}
        onChange={setActiveTab}
        counts={{
          images: pendingFiles.length,
          results: analyzedRecords.length,
        }}
      />

      {detailIndex !== null && (
        <ImageDetail
          records={analyzedRecords}
          initialIndex={detailIndex}
          onClose={() => setDetailIndex(null)}
        />
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}