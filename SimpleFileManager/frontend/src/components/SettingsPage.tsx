import { useEffect, useRef, useState } from 'react';
import { fetchSettings, updateSettings, testConnections, createProvider as apiCreateProvider, createEmbeddingProvider as apiCreateEmbProvider } from '../api';
import { changePassword, checkAuthStatus } from '../auth';
import { useToast } from './ui/Toast';
import { Icon } from './ui/Icon';
import type { AppSettings, EmbeddingProvider, EmbeddingProviderCreate, EmbeddingProviderUpdate, LLMProvider, LLMProviderCreate, LLMProviderUpdate } from '../types';
import { useProviders } from '../hooks/useProviders';
import { useEmbeddingProviders } from '../hooks/useEmbeddingProviders';

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
        className={`relative flex-shrink-0 rounded-full transition-colors
          after:content-[''] after:absolute after:-inset-y-2.5 after:-inset-x-2.5 ${checked ? 'bg-indigo-600' : 'bg-slate-300'}`}
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

/**
 * Inline editor for one provider row. Keeps its own draft so the user
 * can cancel mid-edit without polluting the list state. The "Set as
 * main" / "Delete" actions live on the row outside the form so the
 * user can always reach them, even while editing.
 */
function ProviderRow({
  provider, onSave, onDelete, onSetMain, canDelete,
}: {
  provider: LLMProvider;
  onSave: (patch: LLMProviderUpdate) => Promise<void>;
  onDelete: () => Promise<void>;
  onSetMain: () => Promise<void>;
  canDelete: boolean;
}) {
  const { toast } = useToast();
  const [editing, setEditing] = useState<boolean>(false);
  const [draft, setDraft] = useState({
    name: provider.name,
    base_url: provider.base_url,
    model: provider.model,
    api_key: provider.api_key,
    max_tokens: provider.max_tokens ?? '',
  });
  const [busy, setBusy] = useState(false);

  const handleSave = async () => {
    if (!draft.name.trim() || !draft.base_url.trim() || !draft.model.trim()) {
      toast('名称 / Base URL / 模型名称 不能为空', 'error');
      return;
    }
    setBusy(true);
    try {
      await onSave({
        name: draft.name.trim(),
        base_url: draft.base_url.trim(),
        model: draft.model.trim(),
        api_key: draft.api_key,
        max_tokens: draft.max_tokens === '' ? undefined : Number(draft.max_tokens),
      });
      setEditing(false);
    } catch (e) {
      toast(e instanceof Error ? e.message : '保存失败', 'error');
    } finally { setBusy(false); }
  };

  return (
    <div className={`rounded-lg border ${provider.is_main ? 'border-indigo-300 bg-indigo-50/30' : 'border-slate-200 bg-white'} p-3`}>
      {editing ? (
        <div className="space-y-2">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
            <input className={inputClass} placeholder="名称（仅显示用）" value={draft.name} onChange={e => setDraft({ ...draft, name: e.target.value })} />
            <input className={inputClass} placeholder="模型名称" value={draft.model} onChange={e => setDraft({ ...draft, model: e.target.value })} />
          </div>
          <input className={inputClass} placeholder="Base URL（OpenAI 兼容）" value={draft.base_url} onChange={e => setDraft({ ...draft, base_url: e.target.value })} />
          <input type="password" className={inputClass} placeholder="API Key（本地可留空）" value={draft.api_key} onChange={e => setDraft({ ...draft, api_key: e.target.value })} />
          <input type="number" className={inputClass} placeholder="最大生成 tokens（留空使用系统上限）" value={draft.max_tokens} onChange={e => setDraft({ ...draft, max_tokens: e.target.value })} min={1} />
          <div className="flex items-center gap-2">
            <button onClick={handleSave} disabled={busy}
              className="px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-700 disabled:opacity-40 transition-colors">
              {busy ? '保存中...' : '保存'}
            </button>
            <button onClick={() => { setEditing(false); setDraft({ name: provider.name, base_url: provider.base_url, model: provider.model, api_key: provider.api_key, max_tokens: provider.max_tokens ?? '' }); }}
              className="px-3 py-1.5 rounded-lg bg-slate-100 text-slate-600 text-xs font-medium hover:bg-slate-200 transition-colors">
              取消
            </button>
          </div>
        </div>
      ) : (
        <div className="flex items-start gap-3">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span className="font-medium text-sm text-slate-800 truncate">{provider.name}</span>
              {provider.is_main && (
                <span className="text-[10px] px-1.5 py-0.5 rounded bg-indigo-100 text-indigo-700 font-medium flex-shrink-0">MAIN</span>
              )}
              {provider.max_tokens && (
                <span className="text-[10px] px-1.5 py-0.5 rounded bg-slate-100 text-slate-500 font-medium flex-shrink-0" title={`上限 ${provider.max_tokens} tokens`}>
                  max {provider.max_tokens}
                </span>
              )}
            </div>
            <div className="text-xs text-slate-500 font-mono truncate mt-0.5">{provider.model}</div>
            <div className="text-[11px] text-slate-400 font-mono truncate mt-0.5">{provider.base_url}</div>
          </div>
          <div className="flex items-center gap-1 flex-shrink-0">
            {!provider.is_main && (
              <button onClick={async () => { try { await onSetMain(); toast(`已将 ${provider.name} 设为 main`, 'success'); } catch (e) { toast(e instanceof Error ? e.message : '操作失败', 'error'); } }}
                className="px-2 py-1 rounded text-xs text-indigo-600 hover:bg-indigo-50 transition-colors">
                设为 main
              </button>
            )}
            <button onClick={() => setEditing(true)} className="px-2 py-1 rounded text-xs text-slate-600 hover:bg-slate-100 transition-colors">
              编辑
            </button>
            <button
              onClick={async () => {
                if (!canDelete) {
                  toast('这是唯一的 provider，不能删除', 'error');
                  return;
                }
                if (!window.confirm(`删除 provider "${provider.name}"?关联的会话会回退到 main。`)) return;
                try { await onDelete(); toast('已删除', 'success'); } catch (e) { toast(e instanceof Error ? e.message : '删除失败', 'error'); }
              }}
              disabled={!canDelete}
              className="px-2 py-1 rounded text-xs text-red-500 hover:bg-red-50 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
              title={canDelete ? '删除' : '唯一的 provider 不能删除'}
            >
              删除
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function AddProviderForm({ onAdded, onCancel }: { onAdded: (p: LLMProvider) => void; onCancel: () => void }) {
  const { toast } = useToast();
  const [draft, setDraft] = useState<LLMProviderCreate>({ name: '', base_url: '', model: '', api_key: '', max_tokens: null as number | null });
  const [busy, setBusy] = useState(false);

  const handleAdd = async () => {
    if (!draft.name.trim() || !draft.base_url.trim() || !draft.model.trim()) {
      toast('名称 / Base URL / 模型名称 不能为空', 'error');
      return;
    }
    setBusy(true);
    try {
      const created = await apiCreateProvider({
        name: draft.name.trim(), base_url: draft.base_url.trim(), model: draft.model.trim(), api_key: draft.api_key, max_tokens: draft.max_tokens,
      });
      onAdded(created);
    } catch (e) {
      toast(e instanceof Error ? e.message : '创建失败', 'error');
    } finally { setBusy(false); }
  };

  return (
    <div className="rounded-lg border border-dashed border-slate-300 bg-slate-50/50 p-3 space-y-2">
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
        <input className={inputClass} placeholder="名称（仅显示用，如 Main / 350M）" value={draft.name} onChange={e => setDraft({ ...draft, name: e.target.value })} autoFocus />
        <input className={inputClass} placeholder="模型名称" value={draft.model} onChange={e => setDraft({ ...draft, model: e.target.value })} />
      </div>
      <input className={inputClass} placeholder="Base URL（OpenAI 兼容，如 http://127.0.0.1:9010/v1）" value={draft.base_url} onChange={e => setDraft({ ...draft, base_url: e.target.value })} />
      <input type="password" className={inputClass} placeholder="API Key（本地服务可留空）" value={draft.api_key} onChange={e => setDraft({ ...draft, api_key: e.target.value })} />
      <input type="number" className={inputClass} placeholder="最大生成 tokens（留空使用系统上限）" value={draft.max_tokens ?? ''} onChange={e => setDraft({ ...draft, max_tokens: e.target.value ? Number(e.target.value) : null })} min={1} />
      <div className="flex items-center gap-2">
        <button onClick={handleAdd} disabled={busy}
          className="px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-700 disabled:opacity-40 transition-colors">
          {busy ? '创建中...' : '创建'}
        </button>
        <button onClick={onCancel} className="px-3 py-1.5 rounded-lg bg-slate-100 text-slate-600 text-xs font-medium hover:bg-slate-200 transition-colors">
          取消
        </button>
      </div>
    </div>
  );
}

/**
 * One embedding provider row. Two role buttons ("Set as Index" /
 * "Set as Search") sit on the row next to Edit / Delete, mirroring
 * the LLM ProviderRow's "Set as main" pattern. Both roles can be
 * on at once (a single endpoint that serves both the indexer and
 * the search path); Index is required and refused-by-server when
 * the row is the only is_index, while Search is genuinely optional.
 */
function EmbeddingProviderRow({
  provider, canDelete, onSave, onDelete, onSetIndex, onUnsetIndex, onSetSearch, onUnsetSearch,
}: {
  provider: EmbeddingProvider;
  canDelete: boolean;
  onSave: (patch: EmbeddingProviderUpdate) => Promise<void>;
  onDelete: () => Promise<void>;
  onSetIndex: () => Promise<void>;
  onUnsetIndex: () => Promise<void>;
  onSetSearch: () => Promise<void>;
  onUnsetSearch: () => Promise<void>;
}) {
  const { toast } = useToast();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState({
    name: provider.name,
    base_url: provider.base_url,
    model: provider.model,
    dim: provider.dim,
    api_key: provider.api_key,
  });
  const [busy, setBusy] = useState(false);

  const handleSave = async () => {
    if (!draft.name.trim() || !draft.base_url.trim() || !draft.model.trim()) {
      toast('名称 / Base URL / 模型名称 不能为空', 'error');
      return;
    }
    setBusy(true);
    try {
      await onSave({
        name: draft.name.trim(),
        base_url: draft.base_url.trim(),
        model: draft.model.trim(),
        dim: draft.dim.trim() || 'AUTO',
        api_key: draft.api_key,
      });
      setEditing(false);
    } catch (e) {
      toast(e instanceof Error ? e.message : '保存失败', 'error');
    } finally { setBusy(false); }
  };

  const runRole = async (label: string, fn: () => Promise<void>) => {
    try { await fn(); toast(`${provider.name} → ${label}`, 'success'); }
    catch (e) { toast(e instanceof Error ? e.message : '操作失败', 'error'); }
  };

  return (
    <div className={`rounded-lg border ${provider.is_index ? 'border-violet-300 bg-violet-50/30' : 'border-slate-200 bg-white'} p-3`}>
      {editing ? (
        <div className="space-y-2">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
            <input className={inputClass} placeholder="名称（仅显示用）" value={draft.name} onChange={e => setDraft({ ...draft, name: e.target.value })} />
            <input className={inputClass} placeholder="模型名称" value={draft.model} onChange={e => setDraft({ ...draft, model: e.target.value })} />
          </div>
          <input className={inputClass} placeholder="Base URL（OpenAI 兼容）" value={draft.base_url} onChange={e => setDraft({ ...draft, base_url: e.target.value })} />
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
            <input className={inputClass} placeholder="维度（AUTO 或具体数字，如 768）" value={draft.dim} onChange={e => setDraft({ ...draft, dim: e.target.value })} />
            <input type="password" className={inputClass} placeholder="API Key（本地可留空）" value={draft.api_key} onChange={e => setDraft({ ...draft, api_key: e.target.value })} />
          </div>
          <div className="flex items-center gap-2">
            <button onClick={handleSave} disabled={busy}
              className="px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-700 disabled:opacity-40 transition-colors">
              {busy ? '保存中...' : '保存'}
            </button>
            <button onClick={() => { setEditing(false); setDraft({ name: provider.name, base_url: provider.base_url, model: provider.model, dim: provider.dim, api_key: provider.api_key }); }}
              className="px-3 py-1.5 rounded-lg bg-slate-100 text-slate-600 text-xs font-medium hover:bg-slate-200 transition-colors">
              取消
            </button>
          </div>
        </div>
      ) : (
        <div className="flex items-start gap-3">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="font-medium text-sm text-slate-800 truncate">{provider.name}</span>
              {provider.is_index && (
                <span className="text-[10px] px-1.5 py-0.5 rounded bg-violet-100 text-violet-700 font-medium flex-shrink-0">INDEX</span>
              )}
              {provider.is_search && (
                <span className="text-[10px] px-1.5 py-0.5 rounded bg-slate-200 text-slate-700 font-medium flex-shrink-0">SEARCH</span>
              )}
            </div>
            <div className="text-xs text-slate-500 font-mono truncate mt-0.5">{provider.model} <span className="text-slate-400">· dim {provider.dim}</span></div>
            <div className="text-[11px] text-slate-400 font-mono truncate mt-0.5">{provider.base_url}</div>
          </div>
          <div className="flex items-center gap-1 flex-shrink-0 flex-wrap justify-end">
            {provider.is_index ? (
              <button onClick={() => runRole('取消 Index', onUnsetIndex)} className="px-2 py-1 rounded text-xs text-violet-700 hover:bg-violet-50 transition-colors">
                取消 Index
              </button>
            ) : (
              <button onClick={() => runRole('设为 Index', onSetIndex)} className="px-2 py-1 rounded text-xs text-violet-700 hover:bg-violet-50 transition-colors">
                设为 Index
              </button>
            )}
            {provider.is_search ? (
              <button onClick={() => runRole('取消 Search', onUnsetSearch)} className="px-2 py-1 rounded text-xs text-slate-600 hover:bg-slate-100 transition-colors">
                取消 Search
              </button>
            ) : (
              <button onClick={() => runRole('设为 Search', onSetSearch)} className="px-2 py-1 rounded text-xs text-slate-600 hover:bg-slate-100 transition-colors">
                设为 Search
              </button>
            )}
            <button onClick={() => setEditing(true)} className="px-2 py-1 rounded text-xs text-slate-600 hover:bg-slate-100 transition-colors">
              编辑
            </button>
            <button
              onClick={async () => {
                if (!canDelete) {
                  toast('这是唯一的 INDEX provider，不能删除', 'error');
                  return;
                }
                if (!window.confirm(`删除 embedding provider "${provider.name}"?`)) return;
                try { await onDelete(); toast('已删除', 'success'); } catch (e) { toast(e instanceof Error ? e.message : '删除失败', 'error'); }
              }}
              disabled={!canDelete}
              className="px-2 py-1 rounded text-xs text-red-500 hover:bg-red-50 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
              title={canDelete ? '删除' : '唯一的 INDEX provider 不能删除'}
            >
              删除
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function AddEmbeddingProviderForm({ onAdded, onCancel }: { onAdded: (p: EmbeddingProvider) => void; onCancel: () => void }) {
  const { toast } = useToast();
  const [draft, setDraft] = useState<EmbeddingProviderCreate>({ name: '', base_url: '', model: '', dim: 'AUTO', api_key: '' });
  const [busy, setBusy] = useState(false);

  const handleAdd = async () => {
    if (!draft.name.trim() || !draft.base_url.trim() || !draft.model.trim()) {
      toast('名称 / Base URL / 模型名称 不能为空', 'error');
      return;
    }
    setBusy(true);
    try {
      const created = await apiCreateEmbProvider({
        name: draft.name.trim(), base_url: draft.base_url.trim(), model: draft.model.trim(),
        dim: (draft.dim ?? 'AUTO').trim() || 'AUTO', api_key: draft.api_key,
      });
      onAdded(created);
    } catch (e) {
      toast(e instanceof Error ? e.message : '创建失败', 'error');
    } finally { setBusy(false); }
  };

  return (
    <div className="rounded-lg border border-dashed border-slate-300 bg-slate-50/50 p-3 space-y-2">
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
        <input className={inputClass} placeholder="名称（仅显示用，如 Indexer / Search）" value={draft.name} onChange={e => setDraft({ ...draft, name: e.target.value })} autoFocus />
        <input className={inputClass} placeholder="模型名称" value={draft.model} onChange={e => setDraft({ ...draft, model: e.target.value })} />
      </div>
      <input className={inputClass} placeholder="Base URL（OpenAI 兼容，如 http://127.0.0.1:9011/v1/embeddings）" value={draft.base_url} onChange={e => setDraft({ ...draft, base_url: e.target.value })} />
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
        <input className={inputClass} placeholder="维度（AUTO 或具体数字，如 768）" value={draft.dim} onChange={e => setDraft({ ...draft, dim: e.target.value })} />
        <input type="password" className={inputClass} placeholder="API Key（本地可留空）" value={draft.api_key} onChange={e => setDraft({ ...draft, api_key: e.target.value })} />
      </div>
      <div className="flex items-center gap-2">
        <button onClick={handleAdd} disabled={busy}
          className="px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-700 disabled:opacity-40 transition-colors">
          {busy ? '创建中...' : '创建'}
        </button>
        <button onClick={onCancel} className="px-3 py-1.5 rounded-lg bg-slate-100 text-slate-600 text-xs font-medium hover:bg-slate-200 transition-colors">
          取消
        </button>
      </div>
    </div>
  );
}

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
  const [showAddProvider, setShowAddProvider] = useState(false);
  const providersApi = useProviders();
  const embProvidersApi = useEmbeddingProviders();
  const [showAddEmbProvider, setShowAddEmbProvider] = useState(false);
  // Snapshot of the settings as loaded from the server, so the save
  // handler can tell which fields actually changed since page open. The
  // thinking toggle in particular needs a diff because the save
  // endpoint triggers a llama-server restart when it flips — surfacing
  // the diff lets the toast say "restarting" instead of the generic
  // "applied" message.
  const initialSettingsRef = useRef<AppSettings | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const s = await fetchSettings();
        setSettings(s);
        initialSettingsRef.current = s;
      } catch (e) { console.error('Failed:', e); }
    })();
    checkAuthStatus().then(s => setHasPassword(s.has_password)).catch(() => {});
  }, []);

  const set = (patch: Partial<AppSettings>) => { if (settings) setSettings({ ...settings, ...patch }); };

  const handleSave = async () => {
    if (!settings) return;
    setIsSaving(true); setMessage(null);
    try {
      const saved = await updateSettings(settings);
      // Embedding credentials live in the providers table now; nothing
      // to mask back into local state on save.
      setSettings(saved);
      // LFM does not support request-level thinking-off, so flipping the
      // toggle forces a llama-server restart that takes 10-20s while the
      // 14 GB GGUF reloads. Surface that explicitly so the user doesn't
      // think the page hung — the save button was disabled the whole time
      // and the response is what unblocks it.
      const prev = initialSettingsRef.current;
      const thinkingFlipped = prev != null && prev.thinking_enabled !== settings.thinking_enabled;
      if (thinkingFlipped) {
        setMessage(`已保存，llama-server 已重启以${settings.thinking_enabled ? '启用' : '关闭'}思考模式`);
      } else {
        setMessage('已保存并即时生效');
      }
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
      <div className="bg-white border-b border-slate-200 px-4 sm:px-4 sm:px-6 py-3 flex items-center justify-between gap-3 flex-shrink-0">
        <div className="flex items-center gap-2">
          <Icon name="settings" size={20} className="text-indigo-600" />
          <span className="text-base font-semibold text-slate-800">设置</span>
        </div>
        <button onClick={handleSave} disabled={isSaving}
          className="px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors">
          {isSaving ? '保存中...' : '保存设置'}
        </button>
      </div>

      {message && <div className="px-4 sm:px-6 py-2 bg-indigo-50 text-indigo-700 text-sm">{message}</div>}

      <div className="flex-1 overflow-y-auto p-4 sm:p-6">
        <div className="space-y-4">

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
            title="LLM Providers（对话 / Agent / 日报）"
            action={probes?.llm ? <ProbeBadge result={probes.llm} /> : undefined}
          >
            {providersApi.isLoading ? (
              <div className="text-xs text-slate-400">加载中...</div>
            ) : (
              <div className="space-y-3">
                <p className="text-xs text-slate-400">
                  第一个被创建的 provider 自动成为 <strong className="text-slate-500">main</strong>（新对话的默认模型）。
                  聊天顶部下拉可逐条消息切换到任意一个。系统始终至少保留一个 main。
                </p>
                <div className="space-y-2">
                  {providersApi.providers.map(p => (
                    <ProviderRow
                      key={p.id}
                      provider={p}
                      onSave={(patch) => providersApi.update(p.id, patch).then(() => undefined)}
                      onDelete={() => providersApi.remove(p.id).then(() => undefined)}
                      onSetMain={() => providersApi.setMain(p.id).then(() => undefined)}
                      canDelete={!(p.is_main && providersApi.providers.filter(x => x.is_main).length <= 1)}
                    />
                  ))}
                  {providersApi.providers.length === 0 && (
                    <div className="text-xs text-slate-400 px-1">还没有任何 provider，点击下方"添加"创建一个。</div>
                  )}
                </div>
                {showAddProvider ? (
                  <AddProviderForm
                    onAdded={async () => { await providersApi.refresh(); setShowAddProvider(false); }}
                    onCancel={() => setShowAddProvider(false)}
                  />
                ) : (
                  <button
                    onClick={() => setShowAddProvider(true)}
                    className="w-full flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg border border-dashed border-slate-300 text-sm text-slate-500 hover:border-indigo-300 hover:text-indigo-600 transition-colors"
                  >
                    <Icon name="plus" size={14} />
                    添加 provider
                  </button>
                )}
              </div>
            )}
          </Card>

          <Card
            icon="database" tone="text-violet-500"
            title="Embedding Providers（向量化检索）"
            action={
              <div className="flex items-center gap-1.5">
                {probes?.embedding ? <ProbeBadge result={probes.embedding} /> : null}
                {probes?.embedding_search && embProvidersApi.searchProvider && embProvidersApi.indexProvider?.id !== embProvidersApi.searchProvider.id ? (
                  <ProbeBadge result={probes.embedding_search} />
                ) : null}
              </div>
            }
          >
            {embProvidersApi.isLoading ? (
              <div className="text-xs text-slate-400">加载中...</div>
            ) : (
              <div className="space-y-3">
                <p className="text-xs text-slate-400">
                  每个 row 独立选 <strong className="text-slate-500">Index</strong> / <strong className="text-slate-500">Search</strong> 角色
                  （可同时是两者）。系统当前各取一个：Index 给后台索引器，Search 给查询路径；
                  Search 留空时回退到 Index。多个 row 暂不 fanout——同 dim 的两个 Search 实际只跑第一个，
                  多 dim 切换需要重新建索引。
                </p>
                <div className="space-y-2">
                  {embProvidersApi.providers.map(p => (
                    <EmbeddingProviderRow
                      key={p.id}
                      provider={p}
                      canDelete={!(p.is_index && embProvidersApi.providers.filter(x => x.is_index).length <= 1)}
                      onSave={(patch) => embProvidersApi.update(p.id, patch).then(() => undefined)}
                      onDelete={() => embProvidersApi.remove(p.id).then(() => undefined)}
                      onSetIndex={() => embProvidersApi.update(p.id, { is_index: true }).then(() => undefined)}
                      onUnsetIndex={() => embProvidersApi.update(p.id, { is_index: false }).then(() => undefined)}
                      onSetSearch={() => embProvidersApi.update(p.id, { is_search: true }).then(() => undefined)}
                      onUnsetSearch={() => embProvidersApi.update(p.id, { is_search: false }).then(() => undefined)}
                    />
                  ))}
                  {embProvidersApi.providers.length === 0 && (
                    <div className="text-xs text-slate-400 px-1">还没有任何 embedding provider，点击下方"添加"创建一个。</div>
                  )}
                </div>
                {showAddEmbProvider ? (
                  <AddEmbeddingProviderForm
                    onAdded={async () => { await embProvidersApi.refresh(); setShowAddEmbProvider(false); }}
                    onCancel={() => setShowAddEmbProvider(false)}
                  />
                ) : (
                  <button
                    onClick={() => setShowAddEmbProvider(true)}
                    className="w-full flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg border border-dashed border-slate-300 text-sm text-slate-500 hover:border-indigo-300 hover:text-indigo-600 transition-colors"
                  >
                    <Icon name="plus" size={14} />
                    添加 provider
                  </button>
                )}
              </div>
            )}
          </Card>

          {/* These govern how the configured models behave, so they belong with
              the model definitions rather than under indexing. */}
          <Card icon="sparkles" tone="text-emerald-500" title="生成上限">
            <div className="space-y-3">
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <Field label="最大输出 tokens" hint="对话与 Agent 单次回复上限，默认 4096">
                  <input type="number" min={512} step={128} className={inputClass} value={settings.max_output_tokens}
                    onChange={e => set({ max_output_tokens: Math.max(512, Number(e.target.value)) })} />
                </Field>
                <Field label="日报输出 tokens" hint="日报可更长，默认 8192">
                  <input type="number" min={512} step={128} className={inputClass} value={settings.digest_max_tokens}
                    onChange={e => set({ digest_max_tokens: Math.max(512, Number(e.target.value)) })} />
                </Field>
              </div>
              <p className="text-xs text-slate-400">
                不限制时模型会一直生成到自然结束，容易挤掉后续对话的上下文空间。
                注意该上限<strong className="text-slate-500">包含模型的思考过程</strong>，设得过小会导致回复空白，建议不低于 1024。
              </p>
              <div className="pt-2 border-t border-slate-100">
                <Toggle
                  checked={settings.thinking_enabled}
                  onChange={() => set({ thinking_enabled: !settings.thinking_enabled })}
                  label="思考模式（需重启）"
                  hint="关闭后 llama-server 启动时加 --reasoning-budget 0，模型直接出答案不再写 scratchpad；agent 的工具选择与 plan 推理质量会明显下降"
                />
                <p className="text-xs text-amber-600 mt-2">
                  切换后保存会重启 llama-server（约 10-20 秒），期间 9010 端口上的请求会失败。改动持久化到 .env，手动重启也会沿用。
                </p>
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
              <div className="grid grid-cols-1 xs:grid-cols-2 sm:grid-cols-4 gap-3">
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