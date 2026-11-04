import { useCallback, useEffect, useState } from 'react';
import type { ChangeEvent } from 'react';
import { Icon } from './ui/Icon';
import type { Skill, SkillsState } from '../types';
import {
  fetchSkills, setSkillsMaster, toggleSkill, addSkill, removeSkill,
  importSkillZip, importSkillFiles,
} from '../api';

/**
 * Skills are the "how" — methodology the model follows.
 *
 * With the master switch off, the assistant only has the agent's own
 * read-only tools (list, read, find, search, semantic search) plus
 * submit_plan. With it on, the model also gets to write, move, delete
 * and create folders, guided by whatever skills are enabled. A skill is
 * not a fixed pipeline — it is a chunk of instruction that gets injected
 * so the model knows how to approach a task. The model still freely
 * picks which tools to call.
 *
 * Custom skills can be uploaded by the administrator.
 */
export function SkillsPanel({
  readOnly = false,
  onMasterChange,
  addSignal = 0,
}: {
  readOnly?: boolean;
  onMasterChange?: (master: boolean) => void;
  addSignal?: number;
}) {
  const [state, setState] = useState<SkillsState | null>(null);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [showAdd, setShowAdd] = useState(false);
  const [showMenu, setShowMenu] = useState(false);
  const [newSkill, setNewSkill] = useState({ id: '', label: '', instruction: '', description: '' });
  const [masterBusy, setMasterBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setState(await fetchSkills());
    } catch (e) {
      if (e instanceof Error && e.message.includes('403')) {
        setMessage('只有管理员可以管理技能');
      } else {
        setMessage(e instanceof Error ? e.message : '加载失败');
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  useEffect(() => {
    if (addSignal > 0 && !readOnly) {
      setShowAdd(false);
      setShowMenu(true);
    }
  }, [addSignal, readOnly]);

  const onToggleMaster = async (enabled: boolean) => {
    setMasterBusy(true);
    setMessage(null);
    try {
      const r = await setSkillsMaster(enabled);
      setState(prev => prev ? { ...prev, master: r.master } : prev);
      onMasterChange?.(r.master);
      setMessage(enabled ? '技能已开启：模型将获得更多工具和方法' : '技能已关闭：模型只使用基础工具');
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '操作失败');
    } finally {
      setMasterBusy(false);
    }
  };

  const onToggleSkill = async (skill: Skill, enabled: boolean) => {
    setMessage(null);
    try {
      await toggleSkill(skill.id, enabled);
      setState(prev => prev ? {
        ...prev,
        skills: prev.skills.map(s => s.id === skill.id ? { ...s, enabled } : s),
      } : prev);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '操作失败');
    }
  };
  const onAdd = async () => {
    const id = newSkill.id.trim();
    if (!id || !newSkill.label.trim() || !newSkill.instruction.trim()) {
      setMessage('请填写技能 ID、名称和指令');
      return;
    }
    if (!/^[a-z][a-z0-9_]{1,63}$/.test(id)) {
      setMessage('技能 ID 须以小写字母开头，仅含小写字母、数字、下划线，2-64 个字符');
      return;
    }
    setBusy(true);
    setMessage(null);
    try {
      await addSkill({
        id: newSkill.id.trim(),
        label: newSkill.label.trim(),
        instruction: newSkill.instruction.trim(),
        description: newSkill.description.trim(),
      });
      setNewSkill({ id: '', label: '', instruction: '', description: '' });
      setShowAdd(false);
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '添加失败');
    } finally {
      setBusy(false);
    }
  };

  const onImportZip = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    setBusy(true);
    setMessage(null);
    try {
      const info = await importSkillZip(file);
      setShowMenu(false);
      setMessage(`已导入技能「${info.label}」`);
      await load();
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '导入失败');
    } finally {
      setBusy(false);
    }
  };

  const onImportFiles = async (e: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files || []);
    e.target.value = '';
    if (!files.length) return;
    setBusy(true);
    setMessage(null);
    try {
      const info = await importSkillFiles(files);
      setShowMenu(false);
      setMessage(`已导入技能「${info.label}」`);
      await load();
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '导入失败');
    } finally {
      setBusy(false);
    }
  };

  const onRemove = async (skill: Skill) => {
    if (!window.confirm(`删除技能「${skill.label}」？`)) return;
    setMessage(null);
    try {
      await removeSkill(skill.id);
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '删除失败');
    }
  };

  return (
    <div className="space-y-3">
      {message && (
        <div className="text-sm text-indigo-700 bg-indigo-50 rounded-lg px-3 py-2">{message}</div>
      )}
      {readOnly && (
        <div className="text-xs text-amber-700 bg-amber-50 rounded-lg px-3 py-2">
          只读模式 — 只有管理员可以修改技能设置。
        </div>
      )}

      {/* Master switch */}
      <div className="rounded-lg border border-slate-200 bg-white p-3">
        <div className="flex items-center gap-3">
          <Icon name="sparkles" size={16} className={state?.master ? 'text-indigo-500' : 'text-slate-400'} />
          <div className="flex-1">
            <div className="text-sm font-medium text-slate-800">技能总开关</div>
            <div className="text-[11px] text-slate-400 mt-0.5">
              关闭时模型只使用 Agent 自身的只读工具（列出、读取、查找、语义检索）。
              开启后额外暴露写入、移动、删除、创建等工具，并按启用的技能注入方法论指导。
            </div>
          </div>
          <button
            onClick={() => onToggleMaster(!state?.master)}
            disabled={masterBusy || readOnly}
            className={`relative inline-flex h-6 w-11 flex-shrink-0 rounded-full transition-colors ${
              state?.master ? 'bg-indigo-600' : 'bg-slate-300'
            } disabled:opacity-50`}
          >
            <span style={{ transform: `translateX(${state?.master ? '22px' : '2px'})` }}
              className="inline-block h-5 w-5 rounded-full bg-white shadow mt-0.5 transition-transform" />
          </button>
        </div>
      </div>

      {loading ? (
        <div className="text-sm text-slate-400">加载中...</div>
      ) : (
        <div className="space-y-2">
          {(state?.skills ?? []).map(skill => (
            <div key={skill.id} className="rounded-lg border border-slate-200 bg-white p-3">
              <div className="flex items-start gap-2">
                <Icon name={skill.builtin ? 'shield' : 'sparkles'} size={14}
                  className={`mt-0.5 flex-shrink-0 ${skill.builtin ? 'text-slate-400' : 'text-amber-500'}`} />
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium text-slate-800">{skill.label}</span>
                    {skill.builtin && (
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-slate-100 text-slate-500 font-medium">内置</span>
                    )}
                    {skill.source === 'file' && (
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-indigo-50 text-indigo-600 font-medium">文件</span>
                    )}
                  </div>
                  {skill.description && (
                    <div className="text-[11px] text-slate-400 mt-0.5">{skill.description}</div>
                  )}
                  {skill.source === 'file' && (skill.files?.length ?? 0) > 0 && (
                    <div className="text-[10px] text-slate-400 mt-0.5" title={(skill.files || []).join('\n')}>
                      包含 {skill.files!.length} 个文件
                    </div>
                  )}
                </div>
                <div className="flex items-center gap-2 flex-shrink-0">
                  {!skill.builtin && (
                    <button onClick={() => onRemove(skill)}
                      className="text-red-400 hover:text-red-600 text-[11px]">删除</button>
                  )}
                  <button
                    onClick={() => onToggleSkill(skill, !skill.enabled)}
                    disabled={busy || readOnly}
                    className={`relative inline-flex h-5 w-9 flex-shrink-0 rounded-full transition-colors ${
                      skill.enabled ? 'bg-indigo-600' : 'bg-slate-300'
                    } disabled:opacity-50`}
                  >
                    <span style={{ transform: `translateX(${skill.enabled ? '18px' : '2px'})` }}
                      className="inline-block h-4 w-4 rounded-full bg-white shadow mt-0.5 transition-transform" />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* One entry point; the actual method is chosen after clicking. */}
      {showMenu ? (
        <div className="rounded-lg border border-slate-200 bg-white p-3 space-y-2">
          <div className="text-xs font-medium text-slate-600">添加技能</div>
          <div className="grid grid-cols-2 gap-2">
            <label className={`py-2 rounded-lg border border-dashed border-slate-300 text-slate-600 text-xs text-center cursor-pointer hover:bg-slate-50 ${(busy || readOnly) ? 'opacity-50 pointer-events-none' : ''}`}>
              上传 ZIP 技能包
              <input type="file" accept=".zip,application/zip" className="hidden"
                onChange={onImportZip} disabled={busy || readOnly} />
            </label>
            <label className={`py-2 rounded-lg border border-dashed border-slate-300 text-slate-600 text-xs text-center cursor-pointer hover:bg-slate-50 ${(busy || readOnly) ? 'opacity-50 pointer-events-none' : ''}`}>
              上传文件（含 SKILL.md）
              <input type="file" multiple className="hidden"
                onChange={onImportFiles} disabled={busy || readOnly} />
            </label>
          </div>
          <button onClick={() => { setShowMenu(false); setShowAdd(true); }} disabled={readOnly}
            className="w-full py-2 rounded-lg border border-slate-200 text-slate-600 text-xs hover:bg-slate-50 disabled:opacity-50">
            手动填写（一段方法论）
          </button>
          <p className="text-[11px] text-slate-400">
            文件技能是一个目录：<code className="font-mono">SKILL.md</code> 加可选资源。
            系统只在提示里给摘要，模型需要时用 <code className="font-mono">read_skill</code> 按需读取全文。
          </p>
          <div className="flex justify-end">
            <button onClick={() => setShowMenu(false)}
              className="px-3 py-1.5 rounded-lg text-slate-500 text-xs hover:bg-slate-100">取消</button>
          </div>
        </div>
      ) : showAdd ? (
        <div className="rounded-lg border border-slate-200 bg-white p-3 space-y-2">
          <div>
            <label className="text-xs text-slate-500">技能 ID（英文，不可重复）</label>
            <input value={newSkill.id} onChange={e => setNewSkill(p => ({ ...p, id: e.target.value }))}
              className="mt-1 w-full px-2 py-1.5 rounded-lg border border-slate-200 text-sm" placeholder="如 make_ppt" />
          </div>
          <div>
            <label className="text-xs text-slate-500">名称</label>
            <input value={newSkill.label} onChange={e => setNewSkill(p => ({ ...p, label: e.target.value }))}
              className="mt-1 w-full px-2 py-1.5 rounded-lg border border-slate-200 text-sm" placeholder="如 做演示文稿" />
          </div>
          <div>
            <label className="text-xs text-slate-500">描述</label>
            <input value={newSkill.description} onChange={e => setNewSkill(p => ({ ...p, description: e.target.value }))}
              className="mt-1 w-full px-2 py-1.5 rounded-lg border border-slate-200 text-sm" placeholder="可选" />
          </div>
          <div>
            <label className="text-xs text-slate-500">指令（方法论，注入给模型）</label>
            <textarea value={newSkill.instruction} onChange={e => setNewSkill(p => ({ ...p, instruction: e.target.value }))}
              rows={4}
              className="mt-1 w-full px-2 py-1.5 rounded-lg border border-slate-200 text-sm resize-y"
              placeholder="告诉模型这类任务该怎么做、用什么工具、注意什么…" />
          </div>
          <div className="flex justify-end gap-2">
            <button onClick={() => setShowAdd(false)}
              className="px-3 py-1.5 rounded-lg text-slate-500 text-xs hover:bg-slate-100">取消</button>
            <button onClick={onAdd} disabled={busy || readOnly}
              className="px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-xs hover:bg-indigo-700 disabled:opacity-50">保存</button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
