import { useState, useEffect } from "react";
import {
  analyzeImages,
  exportToJson,
  exportToCsv,
  type AnalysisResult,
  type AnalyzerConfig,
} from "./api/photoAnalyzer";
import {
  savePhotos,
  loadPhotos,
  saveResults,
  loadResults,
  clearAllData,
} from "./api/storage";
import { BottomNav } from "./components/BottomNav";
import { Gallery } from "./components/Gallery";
import { ResultsList } from "./components/ResultsList";
import { ImageDetail } from "./components/ImageDetail";
import { Settings } from "./components/Settings";
import type { TabType, FileEntry } from "./types";

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
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [results, setResults] = useState<AnalysisResult[]>([]);
  const [progress, setProgress] = useState({ current: 0, total: 0 });
  const [isAnalyzing, setIsAnalyzing] = useState(false);
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
        const cachedPhotos = await loadPhotos();
        if (cachedPhotos.length > 0) {
          setFiles(cachedPhotos);
        }
        const cachedResults = await loadResults();
        if (cachedResults.length > 0) {
          setResults(cachedResults);
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

  const addFiles = async (entries: FileEntry[]) => {
    setFiles((prev) => {
      const updated = [...prev, ...entries];
      savePhotos(updated, config.maxCacheCount).catch(console.warn);
      return updated;
    });
    showToast(`已添加 ${entries.length} 张图片`);
  };

  const removeFile = (id: string) => {
    setFiles((prev) => {
      const updated = prev.filter((e) => e.id !== id);
      savePhotos(updated, config.maxCacheCount).catch(console.warn);
      return updated;
    });
  };

  const clearFiles = () => {
    setFiles([]);
    setResults([]);
    setProgress({ current: 0, total: 0 });
    savePhotos([], config.maxCacheCount).catch(console.warn);
    saveResults([], config.maxCacheCount).catch(console.warn);
    showToast("已清空");
  };

  const handleAnalyze = async () => {
    if (!config.apiKey) {
      showToast("请先在「设置」中填写 API Key");
      setActiveTab("settings");
      return;
    }
    if (files.length === 0) {
      showToast("请先添加图片");
      return;
    }

    setIsAnalyzing(true);
    setResults([]);
    setProgress({ current: 0, total: files.length });

    const analysisResults = await analyzeImages(
      files.map((f) => f.file),
      config,
      (current, total) => setProgress({ current, total })
    );

    setResults(analysisResults);
    setIsAnalyzing(false);
    const success = analysisResults.filter((r) => r.success).length;
    showToast(`分析完成！${success}/${analysisResults.length} 成功`);
    setActiveTab("results");
    saveResults(analysisResults, config.maxCacheCount).catch(console.warn);
  };

  const handleExportJson = () => {
    if (results.length === 0) return;
    exportToJson(results);
    showToast("已导出 JSON");
  };

  const handleExportCsv = () => {
    if (results.length === 0) return;
    exportToCsv(results);
    showToast("已导出 CSV");
  };

  const handleClearAll = async () => {
    localStorage.clear();
    await clearAllData();
    setConfig(DEFAULT_CONFIG);
    setFiles([]);
    setResults([]);
    setProgress({ current: 0, total: 0 });
    setTheme("light");
    document.documentElement.setAttribute("data-theme", "light");
    showToast("已清空所有数据");
  };

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
            files={files}
            onAdd={addFiles}
            onRemove={removeFile}
            onClear={clearFiles}
            onAnalyze={handleAnalyze}
            isAnalyzing={isAnalyzing}
            hasResults={results.length > 0}
            progress={progress}
            disabled={isAnalyzing}
          />
        )}

        {activeTab === "results" && (
          <ResultsList
            results={results}
            files={files}
            onSelect={(i) => setDetailIndex(i)}
            onExportJson={handleExportJson}
            onExportCsv={handleExportCsv}
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
          images: files.length,
          results: results.filter((r) => r.success).length,
        }}
      />

      {detailIndex !== null && (
        <ImageDetail
          results={results}
          files={files}
          initialIndex={detailIndex}
          onClose={() => setDetailIndex(null)}
        />
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}