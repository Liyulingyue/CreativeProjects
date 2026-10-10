import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { getAgentTools } from '../api';
import type { AgentToolDefinition, AgentToolDisplay } from '../api';
import { Icon } from './ui/Icon';

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
      className={`bg-white rounded-lg border overflow-hidden ${
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
        <div className="border-t border-slate-100 px-4 py-3 bg-slate-50/60">
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

  useEffect(() => {
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

  return (
    <div className="h-full overflow-y-auto bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 flex items-center gap-2">
        <Icon name="code" size={20} className="text-indigo-600" />
        <span className="text-base font-semibold text-slate-800">Agent 能力</span>
      </div>

      {/* Full width with a responsive grid: the cards are prose plus a short
          parameter list, so one very wide column would leave long unreadable
          lines while a narrow one would squeeze the descriptions. */}
      <div className="p-4 sm:p-6 space-y-5">
        {/* The single most important fact on this page, so it leads. */}
        <div className="flex items-start gap-2.5 p-3 rounded-lg bg-emerald-50 border border-emerald-100">
          <Icon name="shield" size={16} className="text-emerald-600 mt-0.5 flex-shrink-0" />
          <div className="text-sm text-emerald-800 leading-relaxed">
            Agent <strong>只能读取你的文件</strong>，无法直接修改、移动或删除任何内容。
            它能做的唯一写操作是提交一份整理计划，需要你在
            <Link to="/organizer" className="underline hover:text-emerald-900">「整理」</Link>
            页审批后才会执行。
          </div>
        </div>

        {error && (
          <div className="text-sm text-red-600 bg-red-50 rounded-lg px-3 py-2">加载失败：{error}</div>
        )}
        {tools === null && !error && (
          <div className="text-sm text-slate-400">加载中...</div>
        )}

        {readOnly.length > 0 && (
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <h2 className="text-xs font-semibold text-slate-400 uppercase tracking-wider">
                只读工具（{readOnly.length}）
              </h2>
              <div className="flex-1 h-px bg-slate-200" />
            </div>
            <div className="grid grid-cols-1 lg:grid-cols-2 2xl:grid-cols-3 gap-3 items-start">
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
            <div className="grid grid-cols-1 lg:grid-cols-2 2xl:grid-cols-3 gap-3 items-start">
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