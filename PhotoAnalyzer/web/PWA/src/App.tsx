import { useState, useRef, useCallback, useEffect } from "react";
import {
  analyzeImages,
  exportToJson,
  exportToCsv,
  type AnalysisResult,
  type AnalyzerConfig,
} from "./api/photoAnalyzer";
import { ResultCard } from "./components/ResultCard";

const DEFAULT_CONFIG: AnalyzerConfig = {
  apiKey: "",
  baseUrl: "https://api.minimaxi.com/v1",
  model: "MiniMax-M3",
  delay: 1000,
};

interface FileEntry {
  file: File;
  thumb?: string;
}

export default function App() {
  const [config, setConfig] = useState<AnalyzerConfig>(() => {
    const saved = localStorage.getItem("photo-analyzer-config");
    return saved ? { ...DEFAULT_CONFIG, ...JSON.parse(saved) } : DEFAULT_CONFIG;
  });
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [results, setResults] = useState<AnalysisResult[]>([]);
  const [progress, setProgress] = useState({ current: 0, total: 0 });
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [showResults, setShowResults] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    return (localStorage.getItem("theme") as "light" | "dark") || "light";
  });
  const [toast, setToast] = useState<string>("");
  const [online, setOnline] = useState(navigator.onLine);

  const fileInputRef = useRef<HTMLInputElement>(null);
  const dropZoneRef = useRef<HTMLDivElement>(null);

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

  const showToast = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(""), 2400);
  };

  const updateConfig = (updates: Partial<AnalyzerConfig>) => {
    const newConfig = { ...config, ...updates };
    setConfig(newConfig);
    localStorage.setItem("photo-analyzer-config", JSON.stringify(newConfig));
  };

  const handleFiles = useCallback((fileList: FileList | null) => {
    if (!fileList) return;
    const imageFiles = Array.from(fileList).filter((f) =>
      f.type.startsWith("image/")
    );
    if (imageFiles.length === 0) {
      showToast("未识别到图片文件");
      return;
    }

    setFiles((prev) => {
      const newEntries: FileEntry[] = [];
      imageFiles.forEach((f) => {
        newEntries.push({ file: f });
        const reader = new FileReader();
        reader.onload = (e) => {
          const thumb = e.target?.result as string;
          setFiles((curr) =>
            curr.map((entry) =>
              entry.file === f ? { ...entry, thumb } : entry
            )
          );
        };
        reader.readAsDataURL(f);
      });
      return [...prev, ...newEntries];
    });
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      dropZoneRef.current?.classList.remove("dragover");
      handleFiles(e.dataTransfer.files);
    },
    [handleFiles]
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    dropZoneRef.current?.classList.add("dragover");
  }, []);

  const handleDragLeave = useCallback(() => {
    dropZoneRef.current?.classList.remove("dragover");
  }, []);

  const getFileStatus = (index: number): string => {
    if (index < progress.current) {
      return results[index]?.success ? "done" : "error";
    }
    if (index === progress.current && isAnalyzing) return "analyzing";
    return "pending";
  };

  const getFileStatusText = (index: number): string => {
    const status = getFileStatus(index);
    const map = {
      done: "完成",
      error: "失败",
      analyzing: "分析中",
      pending: "等待",
    };
    return map[status as keyof typeof map] || "等待";
  };

  const handleAnalyze = async () => {
    if (!config.apiKey) {
      showToast("请先在设置中填写 API Key");
      setShowSettings(true);
      return;
    }
    if (files.length === 0) {
      showToast("请选择图片");
      return;
    }

    setIsAnalyzing(true);
    setResults([]);
    setShowResults(false);
    setProgress({ current: 0, total: files.length });

    const analysisResults = await analyzeImages(
      files.map((f) => f.file),
      config,
      (current, total) => {
        setProgress({ current, total });
      }
    );

    setResults(analysisResults);
    setIsAnalyzing(false);
    setShowResults(true);
    showToast(`分析完成！成功 ${analysisResults.filter((r) => r.success).length}/${analysisResults.length}`);
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

  const clearFiles = () => {
    setFiles([]);
    setResults([]);
    setShowResults(false);
    setProgress({ current: 0, total: 0 });
  };

  return (
    <div className="app">
      <header className="app-header">
        <div className="app-logo">
          <div className="app-logo-icon">📷</div>
          <span>PhotoAnalyzer</span>
        </div>
        <div style={{ display: "flex", gap: 4 }}>
          <button
            className="theme-toggle"
            onClick={() => setTheme(theme === "light" ? "dark" : "light")}
            aria-label="切换主题"
          >
            {theme === "light" ? "🌙" : "☀️"}
          </button>
          <button
            className="theme-toggle"
            onClick={() => setShowSettings(true)}
            aria-label="设置"
          >
            ⚙️
          </button>
        </div>
      </header>

      {!online && (
        <div className="offline-banner">📴 当前离线，分析功能不可用</div>
      )}

      <main className="app-content">
        <div className="card">
          <div className="card-header">
            <div className="card-header-icon">📁</div>
            <span>选择照片</span>
          </div>

          <div
            ref={dropZoneRef}
            className="drop-zone"
            onClick={() => fileInputRef.current?.click()}
            onDrop={handleDrop}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
          >
            <div className="drop-zone-content">
              <span className="drop-zone-icon">🖼️</span>
              <div className="drop-zone-text">点击或拖拽图片到此处</div>
              <div className="drop-zone-hint">
                支持 JPG · PNG · WebP · GIF
              </div>
            </div>
          </div>

          <input
            ref={fileInputRef}
            type="file"
            multiple
            accept="image/*"
            style={{ display: "none" }}
            onChange={(e) => handleFiles(e.target.files)}
          />

          {files.length > 0 && (
            <>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginTop: 16,
                  marginBottom: 8,
                }}
              >
                <span style={{ fontSize: 13, color: "var(--text-muted)" }}>
                  {files.length} 张图片
                </span>
                <button
                  className="btn btn-secondary btn-compact"
                  onClick={clearFiles}
                  disabled={isAnalyzing}
                >
                  清空
                </button>
              </div>
              <div className="file-list">
                {files.map((entry, index) => (
                  <div key={`${entry.file.name}-${index}`} className="file-item">
                    {entry.thumb ? (
                      <img src={entry.thumb} alt="" className="file-thumb" />
                    ) : (
                      <div className="file-thumb" />
                    )}
                    <div className="file-info">
                      <div className="file-name">{entry.file.name}</div>
                      <div className="file-meta">
                        {(entry.file.size / 1024).toFixed(1)} KB
                      </div>
                    </div>
                    <span
                      className={`file-status ${getFileStatus(index)}`}
                    >
                      {getFileStatusText(index)}
                    </span>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>

        {showResults && results.length > 0 && (
          <div className="card">
            <div className="card-header">
              <div className="card-header-icon">📊</div>
              <span>分析结果</span>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              {results.map((result, index) => (
                <ResultCard
                  key={`result-${index}`}
                  result={result}
                  thumb={files[index]?.thumb}
                />
              ))}
            </div>
          </div>
        )}

        {showResults && results.length === 0 && (
          <div className="empty-state">
            <span className="empty-state-icon">📭</span>
            <div>暂无结果</div>
          </div>
        )}
      </main>

      <div className="action-bar">
        <div className="action-bar-content">
          {progress.total > 0 && isAnalyzing && (
            <div className="progress-container" style={{ flex: 1 }}>
              <div className="progress-bar">
                <div
                  className="progress-fill"
                  style={{
                    width: `${(progress.current / progress.total) * 100}%`,
                  }}
                />
              </div>
              <div className="progress-text">
                {progress.current} / {progress.total} ·{" "}
                {Math.round((progress.current / progress.total) * 100)}%
              </div>
            </div>
          )}
          {!(isAnalyzing && progress.total > 0) && (
            <>
              <button
                className="btn btn-primary"
                onClick={handleAnalyze}
                disabled={files.length === 0 || isAnalyzing}
              >
                {isAnalyzing ? (
                  <>
                    <span className="spinner" />
                    分析中
                  </>
                ) : (
                  <>
                    ✨ 开始分析
                  </>
                )}
              </button>
              <button
                className="btn btn-secondary"
                onClick={handleExportJson}
                disabled={results.length === 0}
              >
                JSON
              </button>
              <button
                className="btn btn-secondary"
                onClick={handleExportCsv}
                disabled={results.length === 0}
              >
                CSV
              </button>
            </>
          )}
        </div>
      </div>

      {showSettings && (
        <div
          className="modal-overlay"
          onClick={(e) => {
            if (e.target === e.currentTarget) setShowSettings(false);
          }}
        >
          <div className="modal">
            <div className="modal-handle" />
            <div className="modal-header">
              <div className="modal-title">⚙️ API 设置</div>
            </div>
            <div className="modal-body">
              <div className="form-group">
                <label className="form-label">API Key</label>
                <input
                  type="password"
                  className="form-input"
                  value={config.apiKey}
                  onChange={(e) => updateConfig({ apiKey: e.target.value })}
                  placeholder="请输入你的 API Key"
                />
              </div>
              <div className="form-group">
                <label className="form-label">Base URL</label>
                <input
                  type="text"
                  className="form-input"
                  value={config.baseUrl}
                  onChange={(e) => updateConfig({ baseUrl: e.target.value })}
                />
              </div>
              <div className="form-row">
                <div className="form-group">
                  <label className="form-label">模型</label>
                  <input
                    type="text"
                    className="form-input"
                    value={config.model}
                    onChange={(e) => updateConfig({ model: e.target.value })}
                  />
                </div>
                <div className="form-group">
                  <label className="form-label">请求间隔 (ms)</label>
                  <input
                    type="number"
                    className="form-input"
                    value={config.delay}
                    onChange={(e) =>
                      updateConfig({ delay: parseInt(e.target.value) || 0 })
                    }
                  />
                </div>
              </div>
              <button
                className="btn btn-primary"
                onClick={() => {
                  setShowSettings(false);
                  showToast("设置已保存");
                }}
                style={{ marginTop: 16 }}
              >
                保存
              </button>
            </div>
          </div>
        </div>
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}