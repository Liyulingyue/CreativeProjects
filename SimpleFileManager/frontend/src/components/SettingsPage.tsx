import { useEffect, useState } from 'react';
import { fetchSettings, updateSettings } from '../api';
import type { AppSettings } from '../types';

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label className="block text-sm font-medium text-slate-700 mb-1">{label}</label>
      {children}
      {hint && <div className="text-xs text-slate-400 mt-1">{hint}</div>}
    </div>
  );
}

const inputClass =
  'w-full px-3 py-2 rounded-lg border border-slate-200 bg-slate-50 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent';

export function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        setSettings(await fetchSettings());
      } catch (e) {
        console.error('Failed to load settings:', e);
      }
    })();
  }, []);

  const set = (patch: Partial<AppSettings>) => {
    if (settings) setSettings({ ...settings, ...patch });
  };

  const handleSave = async () => {
    if (!settings) return;
    setIsSaving(true);
    setMessage(null);
    try {
      const saved = await updateSettings(settings);
      // backend masks api keys — keep local editable copy in sync
      setSettings({ ...settings, llm_api_key: saved.llm_api_key, embedding_api_key: saved.embedding_api_key });
      setMessage('✅ 已保存并即时生效');
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '保存失败');
    } finally {
      setIsSaving(false);
    }
  };

  if (!settings) {
    return <div className="flex items-center justify-center h-full text-slate-400">加载中...</div>;
  }

  return (
    <div className="flex flex-col h-full bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-6 py-3 flex items-center justify-between">
        <div className="text-xl font-bold text-indigo-600">⚙️ 设置</div>
        <button
          onClick={handleSave}
          disabled={isSaving}
          className="px-5 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
        >
          {isSaving ? '保存中...' : '保存设置'}
        </button>
      </div>

      {message && <div className="px-6 py-2 bg-indigo-50 text-indigo-700 text-sm">{message}</div>}

      <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-2xl mx-auto space-y-6">
          {/* LLM */}
          <div className="bg-white rounded-2xl border border-slate-200 p-6 space-y-4">
            <div className="text-lg font-semibold text-slate-700">🤖 LLM（对话 / Agent / 日报）</div>
            <Field label="Base URL" hint="OpenAI 兼容接口，如 llama.cpp / Ollama / vLLM / 云端 API">
              <input className={inputClass} value={settings.llm_base_url} onChange={e => set({ llm_base_url: e.target.value })} />
            </Field>
            <Field label="模型名称">
              <input className={inputClass} value={settings.llm_model} onChange={e => set({ llm_model: e.target.value })} />
            </Field>
            <Field label="API Key" hint="留空表示本地服务无需鉴权；显示 *** 表示已配置">
              <input
                type="password"
                className={inputClass}
                value={settings.llm_api_key}
                onChange={e => set({ llm_api_key: e.target.value })}
              />
            </Field>
          </div>

          {/* Embedding */}
          <div className="bg-white rounded-2xl border border-slate-200 p-6 space-y-4">
            <div className="text-lg font-semibold text-slate-700">🧬 Embedding（向量化检索）</div>
            <Field label="Base URL">
              <input className={inputClass} value={settings.embedding_base_url} onChange={e => set({ embedding_base_url: e.target.value })} />
            </Field>
            <Field label="模型名称" hint="推荐 Qwen3-Embedding-0.6B 等本地模型">
              <input className={inputClass} value={settings.embedding_model} onChange={e => set({ embedding_model: e.target.value })} />
            </Field>
            <Field label="API Key">
              <input
                type="password"
                className={inputClass}
                value={settings.embedding_api_key}
                onChange={e => set({ embedding_api_key: e.target.value })}
              />
            </Field>
          </div>

          {/* Index & Agent */}
          <div className="bg-white rounded-2xl border border-slate-200 p-6 space-y-4">
            <div className="text-lg font-semibold text-slate-700">📋 索引与 Agent</div>
            <Field label="自动索引间隔（秒）" hint="两次轮询扫描之间的最小间隔，最低 30 秒">
              <input
                type="number"
                className={inputClass}
                value={settings.index_interval}
                onChange={e => set({ index_interval: Number(e.target.value) })}
              />
            </Field>
            <Field label="防抖窗口（秒）" hint="刚写入的文件延迟到下一轮再索引，避免读到半成品">
              <input
                type="number"
                className={inputClass}
                value={settings.index_debounce_seconds}
                onChange={e => set({ index_debounce_seconds: Number(e.target.value) })}
              />
            </Field>
            <Field label="Agent 最大步数" hint="单次 Agent 对话中「思考→调工具」的最大循环次数">
              <input
                type="number"
                className={inputClass}
                value={settings.max_agent_steps}
                onChange={e => set({ max_agent_steps: Number(e.target.value) })}
              />
            </Field>
            <div className="flex items-center justify-between p-3 bg-slate-50 rounded-xl">
              <div>
                <div className="text-sm font-medium text-slate-700">自动索引</div>
                <div className="text-xs text-slate-400">后台轮询扫描并自动向量化新文件</div>
              </div>
              <button
                onClick={() => set({ auto_index_enabled: !settings.auto_index_enabled })}
                className={`relative w-11 h-6 rounded-full transition-colors ${
                  settings.auto_index_enabled ? 'bg-indigo-600' : 'bg-slate-300'
                }`}
              >
                <span
                  className={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow transition-transform ${
                    settings.auto_index_enabled ? 'translate-x-5' : ''
                  }`}
                />
              </button>
            </div>
          </div>

          <div className="text-xs text-slate-400 text-center pb-4">
            保存后即时生效，无需重启服务
          </div>
        </div>
      </div>
    </div>
  );
}
