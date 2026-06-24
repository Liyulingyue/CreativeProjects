import { useState } from "react";
import type { AnalysisResult } from "../api/photoAnalyzer";

interface Props {
  result: AnalysisResult;
  thumb?: string;
  onShowDetail?: () => void;
}

export function ResultCard({ result, thumb, onShowDetail }: Props) {
  const [expanded, setExpanded] = useState(false);
  const { data, error, file } = result;

  if (error || !data) {
    return (
      <div className="result-card">
        <div className="result-card-header">
          {thumb && <img src={thumb} alt="" className="result-thumb" />}
          <div className="result-info">
            <div className="result-filename">{file}</div>
            <div className="result-caption">分析失败</div>
          </div>
        </div>
        <div className="result-error">⚠️ {error || "未知错误"}</div>
      </div>
    );
  }

  return (
    <div className="result-card">
      <div className="result-card-header" onClick={onShowDetail}>
        {thumb && <img src={thumb} alt="" className="result-thumb" />}
        <div className="result-info">
          <div className="result-filename">{file}</div>
          <div className="result-caption">{data.caption}</div>
        </div>
        <div className="result-score">{data.score}</div>
      </div>

      {data.main_objects && data.main_objects.length > 0 && (
        <div className="result-tags">
          {data.main_objects.slice(0, 4).map((obj, i) => (
            <span key={i} className="result-tag">
              {obj}
            </span>
          ))}
        </div>
      )}

      <div className="result-body">
        <div className="result-field">
          <div className="result-field-label">风格</div>
          <div className="result-field-value">{data.style || "-"}</div>
        </div>
        <div className="result-field">
          <div className="result-field-label">清晰度</div>
          <div className="result-field-value">{data.blurry || "-"}</div>
        </div>
      </div>

      <div className="result-actions">
        <button
          className="btn btn-secondary btn-compact"
          onClick={(e) => {
            e.stopPropagation();
            setExpanded(!expanded);
          }}
        >
          {expanded ? "收起" : "查看详情"}
        </button>
      </div>

      {expanded && (
        <div className="result-actions">
          <div className="result-field" style={{ flex: 1 }}>
            <div className="result-field-label">详细评价</div>
            <div
              className="result-field-value"
              style={{ whiteSpace: "pre-wrap", lineHeight: 1.5 }}
            >
              {data.comments}
            </div>
            {data.recommendations && (
              <>
                <div
                  className="result-field-label"
                  style={{ marginTop: 10 }}
                >
                  改进建议
                </div>
                <div
                  className="result-field-value"
                  style={{ whiteSpace: "pre-wrap", lineHeight: 1.5 }}
                >
                  {data.recommendations}
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}