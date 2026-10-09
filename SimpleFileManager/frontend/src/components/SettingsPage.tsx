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

function Toggle({ checked, onChange, label, hint }: { checked: boolean; onChange: () => void; label: string; hint: string }) {
  return (
    <div className="flex items-center justify-between p-3 bg-slate-50 rounded-lg">
      <div className="min-w-0">
        <div className="text-sm font-medium text-slate-700">{label}</div>
        <div className="text-xs text-slate-400">{hint}</div>
      </div>
      <button
        onClick={onChange}
        role="switch"
        aria-checked={checked}
        aria-label={label}
        className={`relative flex-shrink-0 rounded-full transition-colors ${checked ? 'bg-indigo-600' : 'bg-slate-300'}`}
        style={{ width: '40px', height: '22px' }}
      >
        <span
          className="absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform"
          style={{ transform: checked ? 'translateX(18px)' : 'translateX(0)' }}
        />
      </button>
    </div>
  );
}

function Card({ icon, tone, title, children, action }: {
  icon: string; tone: string; title: string; children: React.ReactNode; action?: React.ReactNode;
}) {
  return (
    <section className="bg-white rounded-lg border border-slate-200 p-5">
      <div className="flex items-center justify-between gap-3 mb-4">
        <div className="text-base font-semibold text-slate-700 flex items-center gap-2">
          <Icon name={icon} size={18} className={tone} />
          {title}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

function ProbeBadge({ result }: { result: ProbeResult }) {
  return (
    <div className={`flex items-center gap-1.5 px-2 py-1 rounded-md text-xs ${result.ok ? 'bg-green-50 text-green-700' : 'bg-red-50 text-red-700'}`}
         title={result.detail || (result.ok ? '正常' : '异常')}>
      <Icon name={result.ok ? 'checkCircle' : 'alert'} size={13} />
      <span>{result.ok ? '正常' : '异常'}</span>
      {result.ok && result.latency_ms !== undefined && (
        <span className="text-slate-400">{result.latency_ms}ms</span>
      )}
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

  const handleTestConnection = async () => {
    setIsTesting(true);
    setProbes(null);
    try {
      setProbes(await testConnections());
    } catch (e) {
      toast(e instanceof Error ? e.message : '测试失败', 'error');
    } finally { setIsTesting(false); }
  };

  if (!settings) return <div className="flex items-center justify-center h-full text-slate-400">加载中...</div>;

  const dirty = probes !== null;

  return (
    <div className="flex flex-col h-full bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 flex items-center justify-between gap-3 flex-shrink-0">
        <div className="flex items-center gap-2">
          <Icon name="settings" size={20} className="text-indigo-600" />
          <span className="text-base font-semibold text-slate-800">设置</span>
        </div>
        <button onClick={handleSave} disabled={isSaving}
          className="px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors">
          {isSaving ? '保存中...' : '保存设置'}
        </button>
      </div>

      {message && <div className="px-6 py-2 bg-indigo-50 text-indigo-700 text-sm">{message}</div>}

      <div className="flex-1 overflow-y-auto p-4 sm:p-6">
        {/* Single column with a readable measure: settings are mostly URLs and
            numbers, so a full-bleed two-column grid gave over-wide inputs and
            forced short cards to stretch to match their row partner. */}
        <div className="max-w-3xl mx-auto space-y-4">

          {/* ---- 模型服务 ---- */}
          <div className="flex items-center gap-3">
            <h2 className="text-xs font-semibold text-slate-400 uppercase tracking-wider">模型服务</h2>
            <div className="flex-1 h-px bg-slate-200" />
            <button onClick={handleTestConnection} disabled={isTesting}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-100 text-slate-700 text-xs font-medium hover:bg-slate-200 disabled:opacity-50 transition-colors">
              <Icon name="zap" size={14} />
              {isTesting ? '测试中...' : '测试连接'}
            </button>
          </div>

          <Card
            icon="bot" tone="text-indigo-500"
            title="LLM（对话 / Agent / 日报）"
            action={probes?.llm ? <ProbeBadge result={probes.llm} /> : undefined}
          >
            <div className="space-y-3">
              <Field label="Base URL" hint="OpenAI 兼容接口，本地推理可填 http://127.0.0.1:9010/v1">
                <input className={inputClass} value={settings.llm_base_url} onChange={e => set({ llm_base_url: e.target.value })} />
              </Field>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <Field label="模型名称">
                  <input className={inputClass} value={settings.llm_model} onChange={e => set({ llm_model: e.target.value })} />
                </Field>
                <Field label="API Key" hint="本地服务可留空">
                  <input type="password" className={inputClass} value={settings.llm_api_key} onChange={e => set({ llm_api_key: e.target.value })} />
                </Field>
              </div>
            </div>
          </Card>

          <Card
            icon="database" tone="text-violet-500"
            title="Embedding（向量化检索）"
            action={probes?.embedding ? <ProbeBadge result={probes.embedding} /> : undefined}
          >
            <div className="space-y-3">
              <Field label="Base URL" hint="本地推理可填 http://127.0.0.1:9011/v1">
                <input className={inputClass} value={settings.embedding_base_url} onChange={e => set({ embedding_base_url: e.target.value })} />
              </Field>
              <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                <Field label="模型名称">
                  <input className={inputClass} value={settings.embedding_model} onChange={e => set({ embedding_model: e.target.value })} />
                </Field>
                <Field label="向量维度" hint="AUTO 或具体数字">
                  <input className={inputClass} value={settings.embedding_dim} onChange={e => set({ embedding_dim: e.target.value })} />
                </Field>
                <Field label="API Key" hint="可留空">
                  <input type="password" className={inputClass} value={settings.embedding_api_key} onChange={e => set({ embedding_api_key: e.target.value })} />
                </Field>
              </div>
            </div>
          </Card>

          {/* ---- 索引与自动化 ---- */}
          <div className="flex items-center gap-3 pt-2">
            <h2 className="text-xs font-semibold text-slate-400 uppercase tracking-wider">索引与自动化</h2>
            <div className="flex-1 h-px bg-slate-200" />
          </div>

          <Card icon="clipboard" tone="text-amber-500" title="索引与 Agent">
            <div className="space-y-3">
              <Toggle
                checked={settings.auto_index_enabled}
                onChange={() => set({ auto_index_enabled: !settings.auto_index_enabled })}
                label="自动索引"
                hint="后台轮询扫描并自动向量化新文件"
              />
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                <Field label="扫描间隔" hint="秒，最低 30">
                  <input type="number" min={30} className={inputClass} value={settings.index_interval}
                    onChange={e => set({ index_interval: Number(e.target.value) })} />
                </Field>
                <Field label="防抖窗口" hint="秒">
                  <input type="number" min={0} className={inputClass} value={settings.index_debounce_seconds}
                    onChange={e => set({ index_debounce_seconds: Number(e.target.value) })} />
                </Field>
                <Field label="Agent 步数" hint="单次最大循环">
                  <input type="number" min={1} className={inputClass} value={settings.max_agent_steps}
                    onChange={e => set({ max_agent_steps: Number(e.target.value) })} />
                </Field>
                <Field label="上下文窗口" hint="tokens">
                  <input type="number" min={2000} step={1000} className={inputClass} value={settings.max_context_tokens}
                    onChange={e => set({ max_context_tokens: Math.max(2000, Number(e.target.value)) })} />
                </Field>
              </div>
              <p className="text-xs text-slate-400">
                上下文窗口请填模型实际支持的值（32768 / 131072 / 1048576 …）。系统预留 4096 tokens 给模型回复，
                超出后自动把最旧的对话压缩为摘要。LLMServices/serve.sh 会读取同一配置。
              </p>
            </div>
          </Card>

          <Card icon="newspaper" tone="text-orange-500" title="自动日报">
            <div className="space-y-3">
              <Toggle
                checked={settings.auto_digest_enabled}
                onChange={() => set({ auto_digest_enabled: !settings.auto_digest_enabled })}
                label="自动生成日报"
                hint="扫描当日变动文件，用 LLM 撰写结构化日报"
              />
              {settings.auto_digest_enabled && (
                <>
                  <div className="flex rounded-lg border border-slate-200 overflow-hidden">
                    <button type="button" onClick={() => set({ auto_digest_mode: 'scheduled' })}
                      className={`flex-1 px-4 py-2 text-sm font-medium transition-colors ${settings.auto_digest_mode === 'scheduled' ? 'bg-indigo-600 text-white' : 'bg-white text-slate-600 hover:bg-slate-50'}`}>
                      定时生成
                    </button>
                    <button type="button" onClick={() => set({ auto_digest_mode: 'interval' })}
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
          </Card>

          {/* ---- 安全 ---- */}
          <div className="flex items-center gap-3 pt-2">
            <h2 className="text-xs font-semibold text-slate-400 uppercase tracking-wider">安全</h2>
            <div className="flex-1 h-px bg-slate-200" />
          </div>

          <Card icon="shield" tone="text-green-500" title={hasPassword ? '修改密码' : '设置密码'}>
            <div className="space-y-3">
              {!hasPassword && (
                <div className="text-xs text-slate-500 bg-amber-50 rounded-lg px-3 py-2">
                  当前未设置密码，所有人可直接访问。设置后所有 API 请求需要登录 token。
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
                className="px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
              >
                {isChangingPwd ? '保存中...' : hasPassword ? '更新密码' : '设置密码'}
              </button>
            </div>
          </Card>

          <div className="text-xs text-slate-400 text-center pb-4 pt-2">
            保存后即时生效，无需重启服务
            {dirty && <span className="ml-2 text-indigo-500">连接测试结果不会自动保存</span>}
          </div>
        </div>
      </div>
    </div>
  );
}