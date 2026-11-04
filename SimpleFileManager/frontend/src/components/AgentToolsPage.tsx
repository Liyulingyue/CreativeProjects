import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { getAgentTools, fetchCurrentUser, fetchSkills } from '../api';
import type { AgentToolDefinition, AgentToolDisplay } from '../api';
import type { SkillsState } from '../types';
import { Icon } from './ui/Icon';
import { SkillsPanel } from './SkillsPanel';

function RiskBadge({ risk, label }: { risk?: string; label: string }) {
  const approval = risk === 'approval';
  return (
    <span
      className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium flex-shrink-0 ${
        approval ? 'bg-amber-100 text-amber-700' : 'bg-emerald-50 text-emerald-700'
      }`}
    >
      <Icon name={approval ? 'clipboard' : 'eye'} size={10} />
      {label}
    </span>
  );
}

function ToolCard({ tool, def }: { tool: AgentToolDisplay; def?: AgentToolDefinition }) {
  const { display } = tool;
  const approval = display.risk === 'approval';
  const params = def?.function.parameters?.properties || {};
  const paramNames = Object.keys(params);

  return (
    <section
      className={`bg-white rounded-lg border overflow-hidden flex flex-col h-full ${
        approval ? 'border-amber-200' : 'border-slate-200'
      }`}
    >
      <div className={`px-4 py-3 ${approval ? 'bg-amber-50/60' : ''}`}>
        <div className="flex items-center gap-2 mb-1">
          <span className="text-sm font-semibold text-slate-700">{display.title || tool.name}</span>
          <code className="text-[11px] text-slate-400 font-mono">{tool.name}</code>
          <span className="ml-auto">
            <RiskBadge risk={display.risk} label={tool.risk_label} />
          </span>
        </div>
        <p className="text-sm text-slate-500 leading-relaxed">
          {display.summary || def?.function.description}
        </p>
      </div>

      {paramNames.length > 0 && (
        <div className="border-t border-slate-100 px-4 py-3 bg-slate-50/60 flex-1">
          <div className="text-[11px] font-medium text-slate-400 uppercase tracking-wider mb-2">参数</div>
          <div className="space-y-1.5">
            {paramNames.map(name => {
              const spec = params[name];
              const explain = display.params?.[name] || spec.description;
              return (
                <div key={name} className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-xs">
                  <code className="px-1.5 py-0.5 rounded bg-white border border-slate-200 text-slate-600 font-mono flex-shrink-0">
                    {name}
                  </code>
                  <span className="text-slate-400 font-mono flex-shrink-0">{spec.type}</span>
                  <span className="text-slate-500 leading-relaxed min-w-0">{explain}</span>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </section>
  );
}

export function AgentToolsPage() {
  const [tools, setTools] = useState<AgentToolDisplay[] | null>(null);
  const [definitions, setDefinitions] = useState<AgentToolDefinition[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isAdmin, setIsAdmin] = useState(true);
  const [skillsState, setSkillsState] = useState<SkillsState | null>(null);

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
  const readOnly = (tools || []).filter(t => t.display.risk !== 'approval');
  const approval = (tools || []).filter(t => t.display.risk === 'approval');
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

        {/* Dynamic banner: reflects what the agent can actually do right now. */}
        <div className={`flex items-start gap-2.5 p-3 rounded-lg border ${
          skillsOn
            ? 'bg-indigo-50 border-indigo-100'
            : 'bg-emerald-50 border-emerald-100'
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

        {/* Skills control — comes first because it gates everything below. */}
        <div className="space-y-3">
          <div className="flex items-center gap-2">
            <h2 className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
              技能与方法论
            </h2>
            <div className="flex-1 h-px bg-slate-200" />
          </div>
          <p className="text-xs text-slate-400">
            技能决定 Agent「怎么做」。开启后模型获得额外工具和操作能力；关闭时只保留基础只读工具。
          </p>
          <SkillsPanel readOnly={!isAdmin} />
        </div>

        {/* Tools list — what the agent can actually call. */}
        {tools === null && !error && (
          <div className="text-sm text-slate-400">加载工具列表...</div>
        )}

        {readOnly.length > 0 && (
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <h2 className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
                只读工具（{readOnly.length}）
              </h2>
              <div className="flex-1 h-px bg-slate-200" />
            </div>
            <div className="grid grid-cols-1 lg:grid-cols-2 2xl:grid-cols-3 gap-3">
              {readOnly.map(t => <ToolCard key={t.name} tool={t} def={byName.get(t.name)} />)}
            </div>
          </div>
        )}

        {approval.length > 0 && (
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <h2 className="text-xs font-semibold text-amber-600 uppercase tracking-wider">
                需要审批（{approval.length}）
              </h2>
              <div className="flex-1 h-px bg-amber-200" />
            </div>
            <div className="grid grid-cols-1 lg:grid-cols-2 2xl:grid-cols-3 gap-3">
              {approval.map(t => <ToolCard key={t.name} tool={t} def={byName.get(t.name)} />)}
            </div>
          </div>
        )}

        {tools?.length === 0 && (
          <div className="text-sm text-slate-400">Agent 当前没有可用的工具。</div>
        )}
      </div>
    </div>
  );
}
