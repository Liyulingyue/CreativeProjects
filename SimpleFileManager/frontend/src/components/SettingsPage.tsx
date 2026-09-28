import { useEffect, useState } from 'react';
import { fetchSettings, updateSettings, testConnections } from '../api';
import { changePassword, checkAuthStatus } from '../auth';
import { useToast } from './ui/Toast';
import { Icon } from './ui/Icon';
import type { AppSettings } from '../types';

interface ProbeResult { ok: boolean; detail?: string; latency_ms?: number; }

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
  const { toast } = useToast();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [hasPassword, setHasPassword] = useState(false);
  const [oldPwd, setOldPwd] = useState('');
  const [newPwd, setNewPwd] = useState('');
  const [isChangingPwd, setIsChangingPwd] = useState(false);
  const [probes, setProbes] = useState<Record<string, ProbeResult> | null>(null);
  const [isTesting, setIsTesting] = useState(false);

  const handleTestConnection = async () => {
    setIsTesting(true);
    setProbes(null);
    try {
      setProbes(await testConnections());
    } catch (e) {
      toast(e instanceof Error ? e.message : '测试失败', 'error');
    } finally {
      setIsTesting(false);
    }
  };

  useEffect(() => {
    (async () => {
      try { setSettings(await fetchSettings()); } catch (e) { console.error('Failed:', e); }
    })();
    checkAuthStatus().then(s => setHasPassword(s.has_password)).catch(() => {});
  }, []);

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
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
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
            <div className="text-base font-semibold text-slate-700 flex items-center gap-2"><Icon name="zap" size={18} className="text-yellow-500" /> 连接测试</div>
            <div className="text-xs text-slate-400">测试当前配置的 LLM 与 Embedding 服务是否可达，并校验向量维度是否匹配。</div>
            <button onClick={handleTestConnection} disabled={isTesting}
              className="flex items-center gap-1.5 px-4 py-2 rounded-lg bg-slate-100 text-slate-700 text-sm font-medium hover:bg-slate-200 disabled:opacity-50 transition-colors">
              <Icon name="zap" size={16} />
              {isTesting ? '测试中...' : '测试连接'}
            </button>
            {probes && (
              <div className="space-y-2">
                {Object.entries(probes).map(([name, r]) => (
                  <div key={name} className={`p-3 rounded-lg text-sm ${r.ok ? 'bg-green-50' : 'bg-red-50'}`}>
                    <div className="flex items-center justify-between">
                      <span className={`font-medium ${r.ok ? 'text-green-700' : 'text-red-700'}`}>
                        {name === 'llm' ? 'LLM 对话' : 'Embedding'}：{r.ok ? '正常' : '异常'}
                      </span>
                      {r.latency_ms !== undefined && <span className="text-xs text-slate-500">{r.latency_ms}ms</span>}
                    </div>
                    {r.detail && <div className={`text-xs mt-1 break-all ${r.ok ? 'text-green-600' : 'text-red-600'}`}>{r.detail}</div>}
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="bg-white rounded-lg border border-slate-200 p-5 space-y-3">
            <div className="text-base font-semibold text-slate-700 flex items-center gap-2"><Icon name="clipboard" size={18} className="text-amber-500" /> 索引与 Agent</div>
            <Field label="自动索引间隔（秒）" hint="最低 30 秒"><input type="number" className={inputClass} value={settings.index_interval} onChange={e => set({ index_interval: Number(e.target.value) })} /></Field>
            <Field label="防抖窗口（秒）" hint="避免读到半成品"><input type="number" className={inputClass} value={settings.index_debounce_seconds} onChange={e => set({ index_debounce_seconds: Number(e.target.value) })} /></Field>
            <Field label="Agent 最大步数"><input type="number" className={inputClass} value={settings.max_agent_steps} onChange={e => set({ max_agent_steps: Number(e.target.value) })} /></Field>
            <Field label="模型上下文大小（tokens）" hint="你的模型上下文窗口上限，超出会自动压缩历史。如 32768 / 131072 / 1048576">
              <input type="number" min={2000} step={1000} className={inputClass} value={settings.max_context_tokens}
                onChange={e => set({ max_context_tokens: Math.max(2000, Number(e.target.value)) })} />
            </Field>
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

          {/* Auto Digest */}
          <div className="bg-white rounded-lg border border-slate-200 p-5 space-y-3">
            <div className="text-base font-semibold text-slate-700 flex items-center gap-2">
              <Icon name="newspaper" size={18} className="text-orange-500" />
              自动日报
            </div>
            <div className="flex items-center justify-between p-3 bg-slate-50 rounded-lg">
              <div>
                <div className="text-sm font-medium text-slate-700">自动生成日报</div>
                <div className="text-xs text-slate-400">自动扫描当日变动文件并生成知识日报</div>
              </div>
              <button onClick={() => set({ auto_digest_enabled: !settings.auto_digest_enabled })}
                className={`relative rounded-full transition-colors ${settings.auto_digest_enabled ? 'bg-indigo-600' : 'bg-slate-300'}`}
                style={{ width: '40px', height: '22px' }}>
                <span className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform`}
                  style={{ transform: settings.auto_digest_enabled ? 'translateX(18px)' : 'translateX(0)' }} />
              </button>
            </div>
            {settings.auto_digest_enabled && (
              <>
                <div className="flex rounded-lg border border-slate-200 overflow-hidden">
                  <button type="button"
                    onClick={() => set({ auto_digest_mode: 'scheduled' })}
                    className={`flex-1 px-4 py-2 text-sm font-medium transition-colors ${settings.auto_digest_mode === 'scheduled' ? 'bg-indigo-600 text-white' : 'bg-white text-slate-600 hover:bg-slate-50'}`}>
                    定时生成
                  </button>
                  <button type="button"
                    onClick={() => set({ auto_digest_mode: 'interval' })}
                    className={`flex-1 px-4 py-2 text-sm font-medium transition-colors ${settings.auto_digest_mode === 'interval' ? 'bg-indigo-600 text-white' : 'bg-white text-slate-600 hover:bg-slate-50'}`}>
                    间隔生成
                  </button>
                </div>
                {settings.auto_digest_mode === 'scheduled' ? (
                  <Field label="生成时间（小时，0-23）" hint={`每天 ${settings.auto_digest_hour}:00 自动生成`}>
                    <input type="number" min={0} max={23} className={inputClass} value={settings.auto_digest_hour}
                      onChange={e => set({ auto_digest_hour: Math.max(0, Math.min(23, Number(e.target.value))) })} />
                  </Field>
                ) : (
                  <Field label="间隔时间（小时）" hint={`每 ${settings.auto_digest_interval_hours} 小时自动生成一次`}>
                    <input type="number" min={1} max={168} className={inputClass} value={settings.auto_digest_interval_hours}
                      onChange={e => set({ auto_digest_interval_hours: Math.max(1, Number(e.target.value)) })} />
                  </Field>
                )}
              </>
            )}
          </div>

          {/* Security */}
          <div className="bg-white rounded-lg border border-slate-200 p-5 space-y-3">
            <div className="text-base font-semibold text-slate-700 flex items-center gap-2">
              <Icon name="shield" size={18} className="text-green-500" />
              {hasPassword ? '修改密码' : '设置密码'}
            </div>
            {!hasPassword && (
              <div className="text-xs text-slate-400 bg-amber-50 rounded-lg px-3 py-2">
                当前未设置密码，所有人可直接访问。设置密码后所有 API 请求需要登录。
              </div>
            )}
            {hasPassword && (
              <Field label="旧密码">
                <input type="password" className={inputClass} value={oldPwd} onChange={e => setOldPwd(e.target.value)} />
              </Field>
            )}
            <Field label={hasPassword ? '新密码' : '密码'} hint="至少 4 个字符">
              <input type="password" className={inputClass} value={newPwd} onChange={e => setNewPwd(e.target.value)} />
            </Field>
            <button
              onClick={async () => {
                if (newPwd.length < 4) { toast('密码至少 4 个字符', 'error'); return; }
                setIsChangingPwd(true);
                try {
                  const result = await changePassword(oldPwd, newPwd);
                  setHasPassword(true);
                  setOldPwd(''); setNewPwd('');
                  toast(result.message || '密码已设置', 'success');
                } catch (e) {
                  toast(e instanceof Error ? e.message : '操作失败', 'error');
                } finally { setIsChangingPwd(false); }
              }}
              disabled={isChangingPwd || newPwd.length < 4 || (hasPassword && !oldPwd)}
              className="flex items-center gap-1.5 px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-40 transition-colors"
            >
              {isChangingPwd ? '保存中...' : hasPassword ? '更新密码' : '设置密码'}
            </button>
          </div>

          <div className="text-xs text-slate-400 text-center pb-4">保存后即时生效，无需重启服务</div>
        </div>
      </div>
    </div>
  );
}
