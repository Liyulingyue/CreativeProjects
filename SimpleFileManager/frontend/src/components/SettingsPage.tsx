import { useEffect, useState } from 'react';
import { fetchSettings, updateSettings } from '../api';
import { Icon } from './ui/Icon';
import type { AppSettings } from '../types';

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div>
      <label className="block text-sm font-medium text-slate-700 mb-1">{label}</label>
      {children}
      {hint && <div className="text-xs text-slate-400 mt-1">{hint}</div>}
    </div>
  );
}

const inputClass = 'w-full px-3 py-2 rounded-lg border border-slate-200 bg-slate-50 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all';

export function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => { (async () => { try { setSettings(await fetchSettings()); } catch (e) { console.error('Failed:', e); } })(); }, []);

  const set = (patch: Partial<AppSettings>) => { if (settings) setSettings({ ...settings, ...patch }); };

  const handleSave = async () => {
    if (!settings) return;
    setIsSaving(true); setMessage(null);
    try {
      const saved = await updateSettings(settings);
      setSettings({ ...settings, llm_api_key: saved.llm_api_key, embedding_api_key: saved.embedding_api_key });
      setMessage('已保存并即时生效');
    } catch (e) { setMessage(e instanceof Error ? e.message : '保存失败'); } finally { setIsSaving(false); }
  };

  if (!settings) return <div className="flex items-center justify-center h-full text-slate-400">加载中...</div>;

  return (
    <div className="flex flex-col h-full bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-6 py-3 flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-2">
          <Icon name="settings" size={20} className="text-indigo-600" />
          <span className="text-base font-semibold text-slate-800">设置</span>
        </div>
        <button onClick={handleSave} disabled={isSaving}
          className="flex items-center gap-1.5 px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors">
          {isSaving ? '保存中...' : '保存设置'}
        </button>
      </div>

      {message && <div className="px-6 py-2 bg-indigo-50 text-indigo-700 text-sm">{message}</div>}

      <div className="flex-1 overflow-y-auto p-6">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          <div className="bg-white rounded-lg border border-slate-200 p-5 space-y-3">
            <div className="text-base font-semibold text-slate-700 flex items-center gap-2"><Icon name="bot" size={18} className="text-indigo-500" /> LLM（对话 / Agent / 日报）</div>
            <Field label="Base URL" hint="OpenAI 兼容接口"><input className={inputClass} value={settings.llm_base_url} onChange={e => set({ llm_base_url: e.target.value })} /></Field>
            <Field label="模型名称"><input className={inputClass} value={settings.llm_model} onChange={e => set({ llm_model: e.target.value })} /></Field>
            <Field label="API Key" hint="留空表示本地服务无需鉴权"><input type="password" className={inputClass} value={settings.llm_api_key} onChange={e => set({ llm_api_key: e.target.value })} /></Field>
          </div>

          <div className="bg-white rounded-lg border border-slate-200 p-5 space-y-3">
            <div className="text-base font-semibold text-slate-700 flex items-center gap-2"><Icon name="database" size={18} className="text-violet-500" /> Embedding（向量化检索）</div>
            <Field label="Base URL"><input className={inputClass} value={settings.embedding_base_url} onChange={e => set({ embedding_base_url: e.target.value })} /></Field>
            <Field label="模型名称"><input className={inputClass} value={settings.embedding_model} onChange={e => set({ embedding_model: e.target.value })} /></Field>
            <Field label="API Key"><input type="password" className={inputClass} value={settings.embedding_api_key} onChange={e => set({ embedding_api_key: e.target.value })} /></Field>
          </div>

          <div className="bg-white rounded-lg border border-slate-200 p-5 space-y-3">
            <div className="text-base font-semibold text-slate-700 flex items-center gap-2"><Icon name="clipboard" size={18} className="text-amber-500" /> 索引与 Agent</div>
            <Field label="自动索引间隔（秒）" hint="最低 30 秒"><input type="number" className={inputClass} value={settings.index_interval} onChange={e => set({ index_interval: Number(e.target.value) })} /></Field>
            <Field label="防抖窗口（秒）" hint="避免读到半成品"><input type="number" className={inputClass} value={settings.index_debounce_seconds} onChange={e => set({ index_debounce_seconds: Number(e.target.value) })} /></Field>
            <Field label="Agent 最大步数"><input type="number" className={inputClass} value={settings.max_agent_steps} onChange={e => set({ max_agent_steps: Number(e.target.value) })} /></Field>
            <div className="flex items-center justify-between p-3 bg-slate-50 rounded-lg">
              <div>
                <div className="text-sm font-medium text-slate-700">自动索引</div>
                <div className="text-xs text-slate-400">后台轮询扫描并自动向量化新文件</div>
              </div>
              <button onClick={() => set({ auto_index_enabled: !settings.auto_index_enabled })}
                className={`relative rounded-full transition-colors ${settings.auto_index_enabled ? 'bg-indigo-600' : 'bg-slate-300'}`}
                style={{ width: '40px', height: '22px' }}>
                <span className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform`}
                  style={{ transform: settings.auto_index_enabled ? 'translateX(18px)' : 'translateX(0)' }} />
              </button>
            </div>
          </div>
          <div className="text-xs text-slate-400 text-center pb-4">保存后即时生效，无需重启服务</div>
        </div>
      </div>
    </div>
  );
}
