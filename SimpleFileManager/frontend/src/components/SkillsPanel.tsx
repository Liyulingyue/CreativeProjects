import { useCallback, useEffect, useState } from 'react';
import { Icon } from './ui/Icon';
import type { Skill, SkillsState, ToolState } from '../types';
import {
  fetchSkills, setSkillsMaster, toggleSkill, addSkill, removeSkill,
  toggleTool, toggleAllTools,
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
export function SkillsPanel({ readOnly = false }: { readOnly?: boolean }) {
  const [state, setState] = useState<SkillsState | null>(null);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [showAdd, setShowAdd] = useState(false);
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

  const onToggleMaster = async (enabled: boolean) => {
    setMasterBusy(true);
    setMessage(null);
    try {
      const r = await setSkillsMaster(enabled);
      setState(prev => prev ? { ...prev, master: r.master } : prev);
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
  const onToggleTool = async (tool: ToolState, enabled: boolean) => {
    setMessage(null);
    try {
      await toggleTool(tool.name, enabled);
      setState(prev => prev ? {
        ...prev,
        tools: prev.tools.map(t => t.name === tool.name ? { ...t, enabled } : t),
      } : prev);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '操作失败');
    }
  };

  const onToggleAllTools = async (enabled: boolean) => {
    setMessage(null);
    try {
      await toggleAllTools(enabled);
      setState(prev => prev ? {
        ...prev,
        tools: prev.tools.map(t => ({ ...t, enabled })),
      } : prev);
      setMessage(enabled ? '已启用全部工具' : '已关闭全部工具');
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
                  </div>
                  {skill.description && (
                    <div className="text-[11px] text-slate-400 mt-0.5">{skill.description}</div>
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

      {/* Skill-only tool toggles — only when master is on. Tools the agent
          already provides itself (list/read/semantic search) are omitted:
          their switch would give a false sense of control. */}
      {state?.master && (state?.tools ?? []).some(t => !t.agent_builtin) && (
        <div className="space-y-2">
          <div className="flex items-center justify-between pt-2">
            <span className="text-xs font-semibold text-slate-400">技能附加工具</span>
            <div className="flex gap-2">
              <button onClick={() => onToggleAllTools(true)}
                className="text-[11px] text-indigo-500 hover:text-indigo-700">全部启用</button>
              <button onClick={() => onToggleAllTools(false)}
                className="text-[11px] text-slate-400 hover:text-slate-600">全部关闭</button>
            </div>
          </div>
          <p className="text-[11px] text-slate-400">
            列出目录、读取文件、语义检索等只读工具始终由 Agent 提供，见页面上方「只读工具」。
          </p>
          <div className="rounded-lg border border-slate-200 bg-white divide-y divide-slate-100">
            {(state?.tools ?? []).filter(t => !t.agent_builtin).map(tool => (
              <div key={tool.name} className="flex items-center gap-2 px-3 py-2">
                <span className={`text-[10px] px-1.5 py-0.5 rounded font-medium flex-shrink-0 ${
                  tool.danger === 'high' ? 'bg-red-100 text-red-700'
                  : tool.danger === 'medium' ? 'bg-amber-100 text-amber-700'
                  : tool.danger === 'low' ? 'bg-sky-100 text-sky-700'
                  : 'bg-slate-100 text-slate-500'
                }`}>
                  {tool.danger === 'high' ? '危险' : tool.danger === 'medium' ? '谨慎' : tool.danger === 'low' ? '一般' : '安全'}
                </span>
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-medium text-slate-700">{tool.label}</div>
                  <div className="text-[10px] text-slate-400">{tool.description}</div>
                </div>
                <button
                  onClick={() => onToggleTool(tool, !tool.enabled)}
                  className={`relative inline-flex h-5 w-9 flex-shrink-0 rounded-full transition-colors ${
                    tool.enabled ? 'bg-indigo-600' : 'bg-slate-300'
                  }`}
                >
                  <span style={{ transform: `translateX(${tool.enabled ? '18px' : '2px'})` }}
                    className="inline-block h-4 w-4 rounded-full bg-white shadow mt-0.5 transition-transform" />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Add custom skill */}
      {showAdd ? (
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
      ) : (
        <button onClick={() => setShowAdd(true)}
          className="w-full py-2 rounded-lg border border-dashed border-slate-300 text-slate-500 text-xs hover:bg-slate-50">
          + 添加自定义技能
        </button>
      )}
    </div>
  );
}
