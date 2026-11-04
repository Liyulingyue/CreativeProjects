import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import {
  getAgentTools, fetchCurrentUser, fetchSkills, toggleTool, toggleAllTools,
} from '../api';
import type { AgentToolDefinition, AgentToolDisplay } from '../api';
import type { SkillsState, ToolState } from '../types';
import { Icon } from './ui/Icon';
import { SkillsPanel } from './SkillsPanel';

type Tone = 'read' | 'approval' | 'safe' | 'low' | 'medium' | 'high';

interface UnifiedTool {
  name: string;
  label: string;
  description: string;
  params: Record<string, { type?: string; description?: string }>;
  tone: Tone;
  badge: string;
  source: 'agent' | 'skill';
  enabled: boolean;
}

const TONE_STYLE: Record<Tone, string> = {
  read: 'bg-slate-100 text-slate-500',
  approval: 'bg-amber-100 text-amber-700',
  safe: 'bg-emerald-50 text-emerald-700',
  low: 'bg-sky-100 text-sky-700',
  medium: 'bg-amber-100 text-amber-700',
  high: 'bg-red-100 text-red-700',
};

function DangerBadge({ tone, label }: { tone: Tone; label: string }) {
  return (
    <span className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium flex-shrink-0 ${TONE_STYLE[tone]}`}>
      <Icon name={tone === 'approval' ? 'clipboard' : tone === 'read' || tone === 'safe' ? 'eye' : 'alert'} size={10} />
      {label}
    </span>
  );
}

function ToolCard({
  tool, onToggle, toggling, readOnly,
}: {
  tool: UnifiedTool;
  onToggle?: (enabled: boolean) => void;
  toggling?: boolean;
  readOnly?: boolean;
}) {
  const paramNames = Object.keys(tool.params);
  const toggleable = tool.source === 'skill';

  return (
    <section
      className={`bg-white rounded-lg border overflow-hidden flex flex-col h-full ${
        tool.tone === 'approval' ? 'border-amber-200' : 'border-slate-200'
      }`}
    >
      <div className={`px-4 py-3 ${tool.tone === 'approval' ? 'bg-amber-50/60' : ''}`}>
        <div className="flex items-center gap-2 mb-1">
          <span className="text-sm font-semibold text-slate-700">{tool.label}</span>
          <code className="text-[11px] text-slate-400 font-mono">{tool.name}</code>
          <span className="ml-auto">
            <DangerBadge tone={tool.tone} label={tool.badge} />
          </span>
        </div>
        <p className="text-sm text-slate-500 leading-relaxed">{tool.description}</p>
      </div>

      <div className="border-t border-slate-100 px-4 py-3 bg-slate-50/60 flex-1">
        <div className="flex items-center justify-between mb-2">
          <div className="text-[11px] font-medium text-slate-400 uppercase tracking-wider">参数</div>
          {toggleable ? (
            <button
              onClick={() => onToggle?.(!tool.enabled)}
              disabled={toggling || readOnly}
              className={`relative inline-flex h-5 w-9 flex-shrink-0 rounded-full transition-colors ${
                tool.enabled ? 'bg-indigo-600' : 'bg-slate-300'
              } disabled:opacity-50`}
              title={readOnly ? '只有管理员可以修改' : undefined}
            >
              <span style={{ transform: `translateX(${tool.enabled ? '18px' : '2px'})` }}
                className="inline-block h-4 w-4 rounded-full bg-white shadow mt-0.5 transition-transform" />
            </button>
          ) : (
            <span className="text-[10px] text-slate-400">始终可用</span>
          )}
        </div>
        {paramNames.length > 0 ? (
          <div className="space-y-1.5">
            {paramNames.map(name => {
              const spec = tool.params[name];
              return (
                <div key={name} className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-xs">
                  <code className="px-1.5 py-0.5 rounded bg-white border border-slate-200 text-slate-600 font-mono flex-shrink-0">
                    {name}
                  </code>
                  <span className="text-slate-400 font-mono flex-shrink-0">{spec.type}</span>
                  <span className="text-slate-500 leading-relaxed min-w-0">{spec.description}</span>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="text-xs text-slate-400">无参数</div>
        )}
      </div>
    </section>
  );
}

function agentParams(
  def: AgentToolDefinition | undefined,
  friendly: Record<string, string> | undefined,
): Record<string, { type?: string; description?: string }> {
  const props = def?.function.parameters?.properties || {};
  const out: Record<string, { type?: string; description?: string }> = {};
  for (const [name, spec] of Object.entries(props)) {
    out[name] = { type: spec.type, description: friendly?.[name] || spec.description };
  }
  return out;
}

const DANGER_LABEL: Record<ToolState['danger'], string> = {
  safe: '安全', low: '一般', medium: '谨慎', high: '危险',
};

export function AgentToolsPage() {
  const [tools, setTools] = useState<AgentToolDisplay[] | null>(null);
  const [definitions, setDefinitions] = useState<AgentToolDefinition[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isAdmin, setIsAdmin] = useState(true);
  const [skillsState, setSkillsState] = useState<SkillsState | null>(null);
  const [busyTool, setBusyTool] = useState<string | null>(null);

  useEffect(() => {
    fetchCurrentUser().then(u => setIsAdmin(u.is_admin)).catch(() => setIsAdmin(false));
    fetchSkills().then(setSkillsState).catch(() => {});
    getAgentTools()
      .then(d => {
        setTools(d.display || []);
        setDefinitions(d.definitions || []);
      })
      .catch(e => setError(e instanceof Error ? e.message : '加载失败'));
  }, []);

  const byName = new Map(definitions.map(d => [d.function.name, d]));

  const onToggle = async (name: string, enabled: boolean) => {
    setBusyTool(name);
    try {
      await toggleTool(name, enabled);
      setSkillsState(prev => prev ? {
        ...prev, tools: prev.tools.map(t => t.name === name ? { ...t, enabled } : t),
      } : prev);
    } finally {
      setBusyTool(null);
    }
  };
  const onToggleAll = async (enabled: boolean) => {
    setBusyTool('__all__');
    try {
      await toggleAllTools(enabled);
      setSkillsState(prev => prev ? {
        ...prev, tools: prev.tools.map(t => ({ ...t, enabled })),
      } : prev);
    } finally {
      setBusyTool(null);
    }
  };

  // One inventory: each tool appears exactly once. Agent tools are always
  // available; skill tools are only exposed when the master switch is on.
  const agentNames = new Set((tools || []).map(t => t.name));
  const unified: UnifiedTool[] = (tools || []).map(t => ({
    name: t.name,
    label: t.display.title || t.name,
    description: t.display.summary || byName.get(t.name)?.function.description || '',
    params: agentParams(byName.get(t.name), t.display.params),
    tone: t.display.risk === 'approval' ? 'approval' : 'read',
    badge: t.risk_label,
    source: 'agent',
    enabled: true,
  }));
  if (skillsState?.master) {
    for (const s of skillsState.tools) {
      if (agentNames.has(s.name) || s.agent_builtin) continue;
      unified.push({
        name: s.name,
        label: s.label,
        description: s.description,
        params: s.parameters?.properties || {},
        tone: s.danger,
        badge: DANGER_LABEL[s.danger],
        source: 'skill',
        enabled: s.enabled,
      });
    }
  }

  const hasSkillTools = unified.some(t => t.source === 'skill');
  const skillsOn = skillsState?.master ?? false;

  return (
    <div className="h-full overflow-y-auto bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 flex items-center gap-2">
        <Icon name="code" size={20} className="text-indigo-600" />
        <span className="text-base font-semibold text-slate-800">Agent 能力</span>
      </div>

      <div className="p-4 sm:p-6 space-y-6">
        {error && (
          <div className="text-sm text-red-600 bg-red-50 rounded-lg px-3 py-2">加载失败：{error}</div>
        )}

        <div className={`flex items-start gap-2.5 p-3 rounded-lg border ${
          skillsOn ? 'bg-indigo-50 border-indigo-100' : 'bg-emerald-50 border-emerald-100'
        }`}>
          <Icon
            name={skillsOn ? 'sparkles' : 'shield'}
            size={16}
            className={`mt-0.5 flex-shrink-0 ${skillsOn ? 'text-indigo-600' : 'text-emerald-600'}`}
          />
          <div className={`text-sm leading-relaxed ${skillsOn ? 'text-indigo-800' : 'text-emerald-800'}`}>
            {skillsOn ? (
              <>
                技能已开启：Agent 可以<strong>直接操作文件</strong>（写入、移动、删除），
                具体能做什么取决于下方启用的技能和工具。
              </>
            ) : (
              <>
                Agent <strong>只能读取你的文件</strong>，无法直接修改、移动或删除任何内容。
                它能做的唯一写操作是提交一份整理计划，需要你在
                <Link to="/organizer" className="underline hover:text-emerald-900">「整理」</Link>
                页审批后才会执行。
              </>
            )}
          </div>
        </div>

        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <h2 className="text-xs font-semibold text-slate-400 uppercase tracking-wider">技能与方法论</h2>
            <div className="flex-1 h-px bg-slate-200" />
          </div>
          <p className="text-xs text-slate-400">
            技能决定 Agent「怎么做」。开启后模型获得写操作工具和对应方法论；关闭时只保留基础只读工具。
          </p>
          <SkillsPanel
            readOnly={!isAdmin}
            onMasterChange={master => setSkillsState(prev => prev ? { ...prev, master } : prev)}
          />
        </div>

        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <h2 className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
              工具（{unified.length}）
            </h2>
            <div className="flex-1 h-px bg-slate-200" />
            {hasSkillTools && (
              <div className="flex gap-2">
                <button onClick={() => onToggleAll(true)} disabled={!isAdmin || busyTool !== null}
                  className="text-[11px] text-indigo-500 hover:text-indigo-700 disabled:opacity-40">全部启用</button>
                <button onClick={() => onToggleAll(false)} disabled={!isAdmin || busyTool !== null}
                  className="text-[11px] text-slate-400 hover:text-slate-600 disabled:opacity-40">全部关闭</button>
              </div>
            )}
          </div>
          {!skillsOn && (
            <p className="text-xs text-slate-400">开启技能后，这里会额外出现写入、移动、删除等写操作工具。</p>
          )}
          {tools === null && !error && <div className="text-sm text-slate-400">加载工具列表...</div>}
          {unified.length > 0 && (
            <div className="grid grid-cols-1 lg:grid-cols-2 2xl:grid-cols-3 gap-3">
              {unified.map(t => (
                <ToolCard
                  key={t.name}
                  tool={t}
                  readOnly={!isAdmin}
                  toggling={busyTool === t.name || busyTool === '__all__'}
                  onToggle={enabled => onToggle(t.name, enabled)}
                />
              ))}
            </div>
          )}
          {tools?.length === 0 && (
            <div className="text-sm text-slate-400">Agent 当前没有可用的工具。</div>
          )}
        </div>
      </div>
    </div>
  );
}
