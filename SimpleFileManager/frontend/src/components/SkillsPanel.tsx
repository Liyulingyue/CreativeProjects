import { useCallback, useEffect, useState } from 'react';
import type { ChangeEvent, ReactNode } from 'react';
import { Icon } from './ui/Icon';
import { Sheet } from './ui/Dialog';
import { MarkdownText } from './ui/MarkdownText';
import type { Skill, SkillsState } from '../types';
import {
  fetchSkills, setSkillsMaster, toggleSkill, addSkill, removeSkill, updateSkill,
  importSkillZip, importSkillFiles, getSkillResource, putSkillResource,
  uploadSkillResources, deleteSkillResource, downloadSkillResource,
} from '../api';

interface TreeNode {
  name: string;
  path: string;
  children?: TreeNode[];
}

function buildFileTree(paths: string[]): TreeNode[] {
  const root: TreeNode = { name: '', path: '', children: [] };
  for (const full of paths) {
    const parts = full.split('/');
    let node = root;
    parts.forEach((part, i) => {
      node.children = node.children || [];
      let child = node.children.find(c => c.name === part);
      if (!child) {
        child = { name: part, path: parts.slice(0, i + 1).join('/') };
        if (i < parts.length - 1) child.children = [];
        node.children.push(child);
      }
      node = child;
    });
  }
  const sort = (nodes: TreeNode[]) => {
    nodes.sort((a, b) => {
      const af = !!a.children, bf = !!b.children;
      if (af !== bf) return af ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    nodes.forEach(n => n.children && sort(n.children));
  };
  if (root.children) sort(root.children);
  return root.children || [];
}

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
  const [editing, setEditing] = useState<Skill | null>(null);
  const [editForm, setEditForm] = useState({ label: '', description: '', instruction: '' });
  const [selected, setSelected] = useState('SKILL.md');
  const [resource, setResource] = useState<{ path: string; content: string; binary?: boolean } | null>(null);
  const [preview, setPreview] = useState(false);
  const [editMessage, setEditMessage] = useState<string | null>(null);
  const [fileBusy, setFileBusy] = useState(false);
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

  const openEdit = (skill: Skill) => {
    setEditing(skill);
    setEditForm({
      label: skill.label,
      description: skill.description || '',
      instruction: skill.instruction || '',
    });
    setSelected('SKILL.md');
    setResource(null);
    setPreview(false);
    setEditMessage(null);
  };

  const refreshEditing = async (skillId: string) => {
    const fresh = await fetchSkills();
    setState(fresh);
    const found = fresh.skills.find(x => x.id === skillId);
    if (found) setEditing(found);
  };

  const selectFile = async (path: string) => {
    if (!editing) return;
    setSelected(path);
    setResource(null);
    setEditMessage(null);
    if (path === 'SKILL.md') {
      setPreview(false);
      return;
    }
    setFileBusy(true);
    try {
      const r = await getSkillResource(editing.id, path);
      setResource({ path: r.path, content: r.content });
      setPreview(/\.(md|markdown)$/i.test(path));
    } catch (e) {
      const msg = e instanceof Error ? e.message : '读取资源失败';
      if (msg.includes('二进制')) {
        setResource({ path, content: '', binary: true });
        setPreview(false);
      } else {
        setEditMessage(msg);
      }
    } finally {
      setFileBusy(false);
    }
  };

  const onUploadResources = async (e: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files || []);
    e.target.value = '';
    if (!editing || !files.length) return;
    setFileBusy(true);
    setEditMessage(null);
    try {
      const r = await uploadSkillResources(editing.id, files);
      await refreshEditing(editing.id);
      setEditMessage(`已上传 ${r.count} 个文件`);
    } catch (err) {
      setEditMessage(err instanceof Error ? err.message : '上传资源失败');
    } finally {
      setFileBusy(false);
    }
  };

  const onCreateFile = async () => {
    if (!editing) return;
    const path = window.prompt('新建文件（可含子目录，如 notes/demo.md）')?.trim();
    if (!path) return;
    setFileBusy(true);
    setEditMessage(null);
    try {
      await putSkillResource(editing.id, path, '');
      await refreshEditing(editing.id);
      await selectFile(path);
    } catch (e) {
      setEditMessage(e instanceof Error ? e.message : '新建失败');
    } finally {
      setFileBusy(false);
    }
  };

  const onSaveResource = async () => {
    if (!editing || !resource || resource.binary) return;
    setFileBusy(true);
    setEditMessage(null);
    try {
      await putSkillResource(editing.id, resource.path, resource.content);
      setEditMessage('已保存');
    } catch (e) {
      setEditMessage(e instanceof Error ? e.message : '保存资源失败');
    } finally {
      setFileBusy(false);
    }
  };

  const onDeleteResource = async (path: string) => {
    if (!editing || !window.confirm(`删除文件「${path}」？`)) return;
    setFileBusy(true);
    setEditMessage(null);
    try {
      await deleteSkillResource(editing.id, path);
      if (selected === path) {
        setSelected('SKILL.md');
        setResource(null);
      }
      await refreshEditing(editing.id);
    } catch (e) {
      setEditMessage(e instanceof Error ? e.message : '删除资源失败');
    } finally {
      setFileBusy(false);
    }
  };

  const onDownload = async (path: string) => {
    if (!editing) return;
    setEditMessage(null);
    try {
      await downloadSkillResource(editing.id, path);
    } catch (e) {
      setEditMessage(e instanceof Error ? e.message : '下载失败');
    }
  };

  const onSaveEdit = async () => {
    if (!editing) return;
    if (!editForm.label.trim() || !editForm.instruction.trim()) {
      setEditMessage('名称和指令不能为空');
      return;
    }
    setBusy(true);
    setEditMessage(null);
    try {
      await updateSkill(editing.id, {
        label: editForm.label.trim(),
        description: editForm.description.trim(),
        instruction: editForm.instruction,
      });
      await refreshEditing(editing.id);
      setEditMessage('已保存');
    } catch (e) {
      setEditMessage(e instanceof Error ? e.message : '保存失败');
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

  const treeFiles = editing
    ? Array.from(new Set([...(editing.files || []), 'SKILL.md']))
    : [];
  const fileTree = buildFileTree(treeFiles);

  const renderTree = (nodes: TreeNode[], depth: number): ReactNode => nodes.map(node => {
    if (node.children) {
      return (
        <div key={node.path}>
          <div className="flex items-center gap-1 py-1 text-[11px] font-medium text-slate-500"
            style={{ paddingLeft: depth * 10 + 4 }}>
            <Icon name="folder" size={12} />
            <span className="truncate">{node.name}</span>
          </div>
          {renderTree(node.children, depth + 1)}
        </div>
      );
    }
    const active = selected === node.path;
    return (
      <button key={node.path} onClick={() => selectFile(node.path)}
        style={{ paddingLeft: depth * 10 + 4 }}
        className={`w-full flex items-center gap-1.5 py-1 pr-1 rounded text-[11px] text-left ${
          active ? 'bg-indigo-50 text-indigo-700' : 'text-slate-600 hover:bg-slate-50'
        }`}>
        <Icon name={node.path === 'SKILL.md' ? 'sparkles' : 'file'} size={12}
          className={active ? 'text-indigo-500' : 'text-slate-400'} />
        <span className="truncate" title={node.path}>{node.name}</span>
      </button>
    );
  });

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
                    <>
                      <button onClick={() => openEdit(skill)}
                        className="text-slate-400 hover:text-slate-700 text-[11px]">编辑</button>
                      <button onClick={() => onRemove(skill)}
                        className="text-red-400 hover:text-red-600 text-[11px]">删除</button>
                    </>
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

      <Sheet
        open={showMenu || showAdd}
        title={showAdd ? '手动填写技能' : '添加技能'}
        onClose={() => { setShowMenu(false); setShowAdd(false); }}
      >
        {showAdd ? (
          <div className="space-y-3">
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
                rows={5}
                className="mt-1 w-full px-2 py-1.5 rounded-lg border border-slate-200 text-sm resize-y"
                placeholder="告诉模型这类任务该怎么做、用什么工具、注意什么…" />
            </div>
            <div className="flex justify-end gap-2">
              <button onClick={() => { setShowAdd(false); setShowMenu(true); }}
                className="px-3 py-1.5 rounded-lg text-slate-500 text-xs hover:bg-slate-100">返回</button>
              <button onClick={onAdd} disabled={busy || readOnly}
                className="px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-xs hover:bg-indigo-700 disabled:opacity-50">保存</button>
            </div>
          </div>
        ) : (
          <div className="space-y-3">
            <label className={`flex items-center gap-3 p-3 rounded-lg border border-slate-200 cursor-pointer hover:bg-slate-50 ${(busy || readOnly) ? 'opacity-50 pointer-events-none' : ''}`}>
              <Icon name="archive" size={18} className="text-indigo-500 flex-shrink-0" />
              <div className="min-w-0">
                <div className="text-sm font-medium text-slate-700">上传 ZIP 技能包</div>
                <div className="text-[11px] text-slate-400">包含 SKILL.md 与资源的压缩包</div>
              </div>
              <input type="file" accept=".zip,application/zip" className="hidden"
                onChange={onImportZip} disabled={busy || readOnly} />
            </label>
            <label className={`flex items-center gap-3 p-3 rounded-lg border border-slate-200 cursor-pointer hover:bg-slate-50 ${(busy || readOnly) ? 'opacity-50 pointer-events-none' : ''}`}>
              <Icon name="folder" size={18} className="text-indigo-500 flex-shrink-0" />
              <div className="min-w-0">
                <div className="text-sm font-medium text-slate-700">上传文件（含 SKILL.md）</div>
                <div className="text-[11px] text-slate-400">选择一个或多个文件，其中必须有一个 SKILL.md</div>
              </div>
              <input type="file" multiple className="hidden"
                onChange={onImportFiles} disabled={busy || readOnly} />
            </label>
            <button onClick={() => { setShowMenu(false); setShowAdd(true); }} disabled={readOnly}
              className="w-full flex items-center gap-3 p-3 rounded-lg border border-slate-200 text-left hover:bg-slate-50 disabled:opacity-50">
              <Icon name="edit" size={18} className="text-indigo-500 flex-shrink-0" />
              <div className="min-w-0">
                <div className="text-sm font-medium text-slate-700">手动填写</div>
                <div className="text-[11px] text-slate-400">直接写一段方法论，适合简单技能</div>
              </div>
            </button>
            <p className="text-[11px] text-slate-400 leading-relaxed">
              文件技能是一个目录：<code className="font-mono">SKILL.md</code>（YAML frontmatter 的 name/description + 正文）加可选资源。
              系统只在提示里给摘要，模型需要时用 <code className="font-mono">read_skill</code> 按需读取全文。
            </p>
          </div>
        )}
      </Sheet>

      <Sheet
        open={editing !== null}
        title={editing ? `编辑技能 · ${editing.label}` : '编辑技能'}
        onClose={() => { setEditing(null); setResource(null); setSelected('SKILL.md'); }}
      >
        {editing && (
          <div className="space-y-3">
            {editMessage && (
              <div className="text-xs text-indigo-700 bg-indigo-50 rounded-lg px-3 py-2">{editMessage}</div>
            )}

            <div className="grid gap-3 sm:grid-cols-[11rem_1fr]">
              <div className="space-y-2">
                <div className="flex flex-wrap gap-1.5">
                  <label className={`text-[11px] px-2 py-1 rounded-md border border-slate-200 cursor-pointer hover:bg-slate-50 ${(fileBusy || readOnly) ? 'opacity-50 pointer-events-none' : ''}`}>
                    上传
                    <input type="file" multiple className="hidden" onChange={onUploadResources} disabled={fileBusy || readOnly} />
                  </label>
                  <label className={`text-[11px] px-2 py-1 rounded-md border border-slate-200 cursor-pointer hover:bg-slate-50 ${(fileBusy || readOnly) ? 'opacity-50 pointer-events-none' : ''}`}>
                    文件夹
                    <input type="file" multiple className="hidden" onChange={onUploadResources} disabled={fileBusy || readOnly}
                      ref={el => { if (el) el.setAttribute('webkitdirectory', ''); }} />
                  </label>
                  <button onClick={onCreateFile} disabled={fileBusy || readOnly}
                    className="text-[11px] px-2 py-1 rounded-md border border-slate-200 hover:bg-slate-50 disabled:opacity-50">新建</button>
                </div>

                <div className="rounded-lg border border-slate-200 p-1 max-h-56 sm:max-h-80 overflow-y-auto">
                  {fileTree.length === 0 ? (
                    <p className="text-[11px] text-slate-400 px-1 py-2">暂无文件</p>
                  ) : renderTree(fileTree, 0)}
                </div>
                <p className="text-[10px] text-slate-400 leading-relaxed">
                  子目录结构会保留。手动填写的技能在首次上传文件后会自动转为文件技能。
                </p>
              </div>

              <div className="min-w-0 space-y-2">
                <div className="flex items-center gap-2">
                  <code className="flex-1 min-w-0 truncate text-[11px] text-slate-600" title={selected}>{selected}</code>
                  {(!(selected === 'SKILL.md') || editing.source === 'file') && (
                    <button onClick={() => onDownload(selected)}
                      className="text-[11px] text-slate-400 hover:text-slate-700">下载</button>
                  )}
                </div>

                {selected === 'SKILL.md' ? (
                  <>
                    <div className="flex items-center gap-1">
                      <button onClick={() => setPreview(false)}
                        className={`text-[11px] px-2 py-0.5 rounded ${preview ? 'text-slate-400' : 'bg-slate-100 text-slate-700'}`}>编辑</button>
                      <button onClick={() => setPreview(true)}
                        className={`text-[11px] px-2 py-0.5 rounded ${preview ? 'bg-slate-100 text-slate-700' : 'text-slate-400'}`}>预览</button>
                    </div>

                    {preview ? (
                      <div className="rounded-lg border border-slate-200 p-3 max-h-80 overflow-y-auto">
                        <pre className="text-[10px] text-slate-400 bg-slate-50 rounded px-2 py-1 mb-2 whitespace-pre-wrap">{`---\nname: ${editForm.label}\ndescription: ${editForm.description}\n---`}</pre>
                        <MarkdownText source="model" className="text-sm text-slate-700">
                          {editForm.instruction || '_（暂无正文）_'}
                        </MarkdownText>
                      </div>
                    ) : (
                      <div className="space-y-2">
                        <div>
                          <label className="text-[11px] text-slate-500">名称</label>
                          <input value={editForm.label} onChange={e => setEditForm(p => ({ ...p, label: e.target.value }))}
                            className="mt-0.5 w-full px-2 py-1.5 rounded-lg border border-slate-200 text-sm" />
                        </div>
                        <div>
                          <label className="text-[11px] text-slate-500">描述</label>
                          <input value={editForm.description} onChange={e => setEditForm(p => ({ ...p, description: e.target.value }))}
                            className="mt-0.5 w-full px-2 py-1.5 rounded-lg border border-slate-200 text-sm" placeholder="可选" />
                        </div>
                        <div>
                          <label className="text-[11px] text-slate-500">正文（方法论，注入给模型）</label>
                          <textarea value={editForm.instruction} onChange={e => setEditForm(p => ({ ...p, instruction: e.target.value }))}
                            rows={9}
                            className="mt-0.5 w-full px-2 py-1.5 rounded-lg border border-slate-200 text-xs resize-y font-mono" />
                        </div>
                      </div>
                    )}

                    <div className="flex justify-end">
                      <button onClick={onSaveEdit} disabled={busy || readOnly}
                        className="px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-xs hover:bg-indigo-700 disabled:opacity-50">
                        {busy ? '保存中…' : '保存 SKILL.md'}
                      </button>
                    </div>
                  </>
                ) : resource ? (
                  resource.binary ? (
                    <div className="rounded-lg border border-slate-200 p-6 text-center space-y-2">
                      <Icon name="file" size={22} className="text-slate-300 mx-auto" />
                      <p className="text-xs text-slate-500">二进制文件，无法在线预览或编辑。</p>
                      <button onClick={() => onDownload(resource.path)}
                        className="px-3 py-1.5 rounded-lg border border-slate-200 text-xs hover:bg-slate-50">下载文件</button>
                    </div>
                  ) : (
                    <>
                      <div className="flex items-center gap-1">
                        <button onClick={() => setPreview(false)}
                          className={`text-[11px] px-2 py-0.5 rounded ${preview ? 'text-slate-400' : 'bg-slate-100 text-slate-700'}`}>编辑</button>
                        <button onClick={() => setPreview(true)}
                          className={`text-[11px] px-2 py-0.5 rounded ${preview ? 'bg-slate-100 text-slate-700' : 'text-slate-400'}`}>预览</button>
                      </div>

                      {preview ? (
                        <div className="rounded-lg border border-slate-200 p-3 max-h-80 overflow-y-auto">
                          {/\.(md|markdown)$/i.test(resource.path) ? (
                            <MarkdownText source="model" className="text-sm text-slate-700">{resource.content}</MarkdownText>
                          ) : (
                            <pre className="text-[11px] whitespace-pre-wrap break-words text-slate-700">{resource.content}</pre>
                          )}
                        </div>
                      ) : (
                        <textarea value={resource.content}
                          onChange={e => setResource(r => r ? { ...r, content: e.target.value } : r)}
                          rows={14}
                          className="w-full px-2 py-1.5 rounded-lg border border-slate-200 text-xs font-mono resize-y" />
                      )}

                      <div className="flex items-center justify-between">
                        <button onClick={() => onDeleteResource(resource.path)} disabled={fileBusy || readOnly}
                          className="text-[11px] text-red-500 hover:text-red-700 disabled:opacity-40">删除此文件</button>
                        <button onClick={onSaveResource} disabled={fileBusy || readOnly || preview}
                          className="px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-xs hover:bg-indigo-700 disabled:opacity-50">
                          {fileBusy ? '保存中…' : '保存'}
                        </button>
                      </div>
                    </>
                  )
                ) : (
                  <div className="text-xs text-slate-400 px-1 py-4">{fileBusy ? '读取中…' : '从左侧选择文件'}</div>
                )}
              </div>
            </div>

            <p className="text-[11px] text-slate-400">技能 ID 固定为 <code className="font-mono">{editing.id}</code>，不可修改。</p>
            <div className="flex justify-end">
              <button onClick={() => { setEditing(null); setResource(null); setSelected('SKILL.md'); }}
                className="px-3 py-1.5 rounded-lg bg-slate-100 text-slate-700 text-xs hover:bg-slate-200">完成</button>
            </div>
          </div>
        )}
      </Sheet>
    </div>
  );
}
