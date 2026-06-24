import { useState, useRef, useCallback } from "react";
import {
  analyzeImages,
  exportToJson,
  exportToCsv,
  type AnalysisResult,
  type AnalyzerConfig,
} from "./api/photoAnalyzer";

const DEFAULT_CONFIG: AnalyzerConfig = {
  apiKey: "",
  baseUrl: "https://api.minimaxi.com/v1",
  model: "MiniMax-M3",
  delay: 1000,
};

export default function App() {
  const [config, setConfig] = useState<AnalyzerConfig>(() => {
    const saved = localStorage.getItem("photo-analyzer-config");
    return saved ? { ...DEFAULT_CONFIG, ...JSON.parse(saved) } : DEFAULT_CONFIG;
  });
  const [files, setFiles] = useState<File[]>([]);
  const [results, setResults] = useState<AnalysisResult[]>([]);
  const [progress, setProgress] = useState({ current: 0, total: 0 });
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [showResults, setShowResults] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const dropZoneRef = useRef<HTMLDivElement>(null);

  const updateConfig = (updates: Partial<AnalyzerConfig>) => {
    const newConfig = { ...config, ...updates };
    setConfig(newConfig);
    localStorage.setItem("photo-analyzer-config", JSON.stringify(newConfig));
  };

  const handleFiles = useCallback((fileList: FileList | null) => {
    if (!fileList) return;
    const imageFiles = Array.from(fileList).filter((f) => f.type.startsWith("image/"));
    setFiles((prev) => [...prev, ...imageFiles]);
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    dropZoneRef.current?.classList.remove("dragover");
    handleFiles(e.dataTransfer.files);
  }, [handleFiles]);

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
    switch (status) {
      case "done":
        return "完成";
      case "error":
        return "失败";
      case "analyzing":
        return "分析中";
      default:
        return "等待";
    }
  };

  const handleAnalyze = async () => {
    if (!config.apiKey) {
      alert("请输入 API Key");
      return;
    }
    if (files.length === 0) {
      alert("请选择图片");
      return;
    }

    setIsAnalyzing(true);
    setResults([]);
    setShowResults(false);
    setProgress({ current: 0, total: files.length });

    const analysisResults = await analyzeImages(files, config, (current, total) => {
      setProgress({ current, total });
    });

    setResults(analysisResults);
    setIsAnalyzing(false);
    setShowResults(true);
  };

  const handleExportJson = () => exportToJson(results);
  const handleExportCsv = () => exportToCsv(results);

  return (
    <div className="container">
      <h1>📷 PhotoAnalyzer</h1>

      <div className="card">
        <h2>⚙️ API 配置</h2>
        <div className="form-group">
          <label>API Key</label>
          <input
            type="password"
            value={config.apiKey}
            onChange={(e) => updateConfig({ apiKey: e.target.value })}
            placeholder="输入你的 API Key"
          />
        </div>
        <div className="flex">
          <div className="form-group flex-1">
            <label>Base URL</label>
            <input
              type="text"
              value={config.baseUrl}
              onChange={(e) => updateConfig({ baseUrl: e.target.value })}
            />
          </div>
          <div className="form-group flex-1">
            <label>模型</label>
            <input
              type="text"
              value={config.model}
              onChange={(e) => updateConfig({ model: e.target.value })}
            />
          </div>
          <div className="form-group flex-1">
            <label>请求间隔 (ms)</label>
            <input
              type="number"
              value={config.delay}
              onChange={(e) => updateConfig({ delay: parseInt(e.target.value) || 0 })}
            />
          </div>
        </div>
      </div>

      <div className="card">
        <h2>📁 选择图片</h2>
        <div
          ref={dropZoneRef}
          className="drop-zone"
          onClick={() => fileInputRef.current?.click()}
          onDrop={handleDrop}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
        >
          <p>拖拽图片到这里或点击选择</p>
          <p className="hint">支持 JPG, PNG, GIF, WebP 格式</p>
        </div>
        <input
          ref={fileInputRef}
          type="file"
          multiple
          accept="image/*"
          className="hidden"
          onChange={(e) => handleFiles(e.target.files)}
        />
        {files.length > 0 && (
          <div className="file-list">
            {files.map((file, index) => (
              <div key={`${file.name}-${index}`} className="file-item">
                <div>
                  <div className="name">{file.name}</div>
                  <div className="size">{(file.size / 1024).toFixed(1)} KB</div>
                </div>
                <span className={`status ${getFileStatus(index)}`}>
                  {getFileStatusText(index)}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="card">
        {progress.total > 0 && (
          <div className="progress-bar">
            <div
              className="fill"
              style={{ width: `${(progress.current / progress.total) * 100}%` }}
            />
          </div>
        )}
        <div className="flex">
          <button
            className="btn btn-primary"
            onClick={handleAnalyze}
            disabled={isAnalyzing || files.length === 0}
          >
            {isAnalyzing ? "分析中..." : "开始分析"}
          </button>
          <button
            className="btn btn-secondary"
            onClick={handleExportJson}
            disabled={results.length === 0}
          >
            导出 JSON
          </button>
          <button
            className="btn btn-secondary"
            onClick={handleExportCsv}
            disabled={results.length === 0}
          >
            导出 CSV
          </button>
        </div>
      </div>

      {showResults && results.length > 0 && (
        <div className="card">
          <h2>📊 分析结果</h2>
          <div className="results">
            {results.map((result, index) => (
              <div key={`result-${index}`} className="result-item">
                <div className="header">
                  <span className="filename">{result.file}</span>
                  {result.data?.score !== undefined && (
                    <span className="score">{result.data.score}</span>
                  )}
                </div>
                {result.error && <p className="error-text">错误: {result.error}</p>}
                {result.data && (
                  <div className="json-view">{JSON.stringify(result.data, null, 2)}</div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
