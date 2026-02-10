import { useState, useEffect } from 'react';
import axios from 'axios';

interface Version {
  id: number;
  version_hash: string;
  content_hash: string;
  upload_time: string;
  comment: string;
}

interface HistoryPanelProps {
  filename: string;
  onClose: () => void;
}

export default function HistoryPanel({ filename, onClose }: HistoryPanelProps) {
  const [versions, setVersions] = useState<Version[]>([]);
  const [diff, setDiff] = useState<string | null>(null);
  const [selectedVersions, setSelectedVersions] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    fetchVersions();
  }, [filename]);

  const fetchVersions = async () => {
    try {
      const response = await axios.get(`http://localhost:8000/versions/${encodeURIComponent(filename)}`);
      setVersions(response.data);
    } catch (error) {
      console.error('Failed to fetch versions:', error);
    }
  };

  const calculateDiff = async () => {
    if (selectedVersions.length !== 2) return;
    setLoading(true);
    try {
      // Backend expects v1 and v2. We'll use the older as v1 and newer as v2 based on their index in the list
      // Since versions are ordered DESC by time, higher index is older.
      const vIndices = selectedVersions.map(hash => versions.findIndex(v => v.content_hash === hash)).sort((a, b) => b - a);
      const v1 = versions[vIndices[0]].content_hash; // older
      const v2 = versions[vIndices[1]].content_hash; // newer
      
      const response = await axios.get(`http://localhost:8000/diff/${encodeURIComponent(filename)}?v1=${v1}&v2=${v2}`);
      setDiff(response.data.diff);
    } catch (error) {
      setDiff("无法计算差异：文件可能不是纯文本或已丢失。");
    } finally {
      setLoading(false);
    }
  };

  const toggleVersionSelection = (hash: string) => {
    if (selectedVersions.includes(hash)) {
      setSelectedVersions(selectedVersions.filter(v => v !== hash));
    } else if (selectedVersions.length < 2) {
      setSelectedVersions([...selectedVersions, hash]);
    }
  };

  return (
    <div className="fixed inset-y-0 right-0 w-[500px] bg-white shadow-2xl z-[60] flex flex-col animate-in slide-in-from-right duration-300 border-l border-slate-100">
      <div className="p-6 border-b border-slate-100 flex items-center justify-between bg-slate-50/50">
        <div>
          <h3 className="text-lg font-black text-slate-800 uppercase tracking-tighter">版本历史</h3>
          <p className="text-[10px] text-slate-400 font-bold truncate max-w-[300px]">{filename}</p>
        </div>
        <button onClick={onClose} className="w-8 h-8 flex items-center justify-center rounded-full hover:bg-slate-200 transition-colors">✕</button>
      </div>

      <div className="flex-1 overflow-y-auto p-6 custom-scrollbar">
        <div className="space-y-4">
          {versions.map((v, idx) => (
            <div 
              key={v.id}
              onClick={() => toggleVersionSelection(v.content_hash)}
              className={`p-4 rounded-2xl border transition-all cursor-pointer ${
                selectedVersions.includes(v.content_hash) 
                  ? 'border-indigo-500 bg-indigo-50/50 ring-1 ring-indigo-500' 
                  : 'border-slate-100 hover:border-indigo-200 bg-white'
              }`}
            >
              <div className="flex justify-between items-start mb-2">
                <span className="text-[10px] font-black text-white bg-indigo-400 px-2 py-0.5 rounded-md uppercase">
                  {idx === 0 ? 'Latest' : `v.${versions.length - idx}`}
                </span>
                <span className="text-[10px] font-black text-slate-300 uppercase leading-none">
                  {new Date(v.upload_time).toLocaleString()}
                </span>
              </div>
              <p className="text-sm font-bold text-slate-700 mb-1">{v.comment || "无备注上传"}</p>
              <p className="text-[9px] font-mono text-slate-400 break-all">{v.content_hash}</p>
            </div>
          ))}
        </div>
      </div>

      {selectedVersions.length === 2 && (
        <div className="p-6 border-t border-slate-100 bg-white">
          <button 
            onClick={calculateDiff}
            className="w-full py-3 bg-indigo-600 text-white font-black text-xs uppercase tracking-widest rounded-xl hover:bg-indigo-700 shadow-xl shadow-indigo-200 transition-all active:scale-95 mb-4"
          >
            {loading ? '分析中...' : '对比选中版本'}
          </button>
          {diff && (
            <pre className="p-4 bg-slate-900 text-indigo-300 text-[10px] rounded-xl overflow-x-auto font-mono whitespace-pre-wrap max-h-[300px] custom-scrollbar">
              {diff || "暂无差异"}
            </pre>
          )}
        </div>
      )}

      {selectedVersions.length < 2 && (
        <div className="p-6 border-t border-slate-100 bg-slate-50 text-center">
          <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest leading-loose">
            选择两个版本以查看内容差异对比<br/>
            (仅限纯文本文件)
          </p>
        </div>
      )}
    </div>
  );
}
