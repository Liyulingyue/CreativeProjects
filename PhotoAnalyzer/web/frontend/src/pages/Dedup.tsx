import { useState, useEffect, useCallback } from "react";
import { startDedup, getDedupJob, resolveDedupGroups } from "@/api/dedup";
import { listDirs } from "@/api/files";
import type { DirEntry, DedupJob, DedupGroup } from "@/api/types";
import { apiUrl } from "@/api/client";

export function Dedup() {
  const [dirs, setDirs] = useState<DirEntry[]>([]);
  const [selectedDir, setSelectedDir] = useState<DirEntry | null>(null);
  const [job, setJob] = useState<DedupJob | null>(null);
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [keepSelections, setKeepSelections] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    listDirs().then(setDirs).catch(() => {});
  }, []);

  const pollJob = useCallback(async (jobId: string) => {
    try {
      const result = await getDedupJob(jobId);
      setJob(result);
      if (result.status === "running" || result.status === "pending") {
        setTimeout(() => pollJob(jobId), 2000);
      }
    } catch {
      setTimeout(() => pollJob(jobId), 3000);
    }
  }, []);

  const handleStart = async () => {
    if (!selectedDir) return;
    setLoading(true);
    try {
      const result = await startDedup(selectedDir.id);
      setJob(result);
      pollJob(result.job_id);
    } catch (e) {
      alert(e instanceof Error ? e.message : "Failed to start dedup");
    } finally {
      setLoading(false);
    }
  };

  const toggleGroup = (groupId: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupId)) next.delete(groupId);
      else next.add(groupId);
      return next;
    });
  };

  const selectKeep = (groupId: string, path: string) => {
    setKeepSelections((prev) => ({ ...prev, [groupId]: path }));
  };

  const handleResolve = async () => {
    if (!job) return;
    const actions = Object.entries(keepSelections).map(([groupId, keep]) => {
      const group = job.groups.find((g) => g.group_id === groupId);
      return {
        group_id: groupId,
        keep,
        remove: group?.items.filter((i) => i.path !== keep).map((i) => i.path) ?? [],
      };
    });

    try {
      await resolveDedupGroups(job.job_id, actions);
      setJob(null);
      setKeepSelections({});
      setExpandedGroups(new Set());
    } catch (e) {
      alert(e instanceof Error ? e.message : "Failed to resolve");
    }
  };

  return (
    <div className="page">
      <h1>照片去重</h1>

      <div className="card">
        <h3>启动去重</h3>
        <div className="form-group">
          <label>选择目录</label>
          <select
            value={selectedDir?.id ?? ""}
            onChange={(e) => {
              const dir = dirs.find((d) => d.id === e.target.value);
              setSelectedDir(dir ?? null);
            }}
          >
            <option value="">-- 选择目录 --</option>
            {dirs.map((d) => (
              <option key={d.id} value={d.id}>{d.name || d.path}</option>
            ))}
          </select>
        </div>
        <button
          className="btn btn--primary"
          onClick={handleStart}
          disabled={loading || !selectedDir || job?.status === "running"}
        >
          {job?.status === "running" ? "去重进行中..." : "开始去重"}
        </button>
      </div>

      {job && job.status === "running" && (
        <div className="card dedup-progress">
          <p>正在去重分析...</p>
          <p>阶段: {job.stage || "—"}</p>
          <p>文件数: {job.total_files}</p>
        </div>
      )}

      {job && (job.status === "completed" || job.status === "running") && job.groups.length > 0 && (
        <div className="dedup-results">
          <div className="section-header">
            <h2>重复组 ({job.groups.length})</h2>
            {job.status === "completed" && Object.keys(keepSelections).length > 0 && (
              <button className="btn btn--primary" onClick={handleResolve}>
                执行清理 ({Object.keys(keepSelections).length} 组)
              </button>
            )}
          </div>

          {job.groups.map((group: DedupGroup) => (
            <div key={group.group_id} className="dedup-group">
              <div
                className="dedup-group__header"
                onClick={() => toggleGroup(group.group_id)}
              >
                <span>
                  组 {group.group_id.slice(0, 8)} — {group.items.length} 张相似
                  {group.stage && ` (${group.stage})`}
                </span>
                <span>{expandedGroups.has(group.group_id) ? "▼" : "▶"}</span>
              </div>

              {expandedGroups.has(group.group_id) && (
                <div className="dedup-group__items">
                  {group.items.map((item) => (
                    <div
                      key={item.path}
                      className={`dedup-item ${keepSelections[group.group_id] === item.path ? "dedup-item--keep" : ""}`}
                      onClick={() => selectKeep(group.group_id, item.path)}
                    >
                      <div className="dedup-item__thumb">
                        {item.thumbnail_url ? (
                          <img src={apiUrl(item.thumbnail_url)} alt={item.file_name} />
                        ) : (
                          <span>📷</span>
                        )}
                      </div>
                      <div className="dedup-item__info">
                        <div>{item.file_name}</div>
                        <div className="dedup-item__meta">
                          {(item.file_size / 1024 / 1024).toFixed(1)} MB
                          {item.similarity > 0 && ` · 相似度 ${(item.similarity * 100).toFixed(0)}%`}
                        </div>
                      </div>
                      <div className="dedup-item__action">
                        {keepSelections[group.group_id] === item.path ? "✓ 保留" : "点击保留"}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {job && job.status === "completed" && job.groups.length === 0 && (
        <div className="card">
          <p>未发现重复照片</p>
        </div>
      )}
    </div>
  );
}
