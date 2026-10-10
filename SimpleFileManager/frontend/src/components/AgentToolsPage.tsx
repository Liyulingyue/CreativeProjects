import { useEffect, useState } from 'react';
import { getAgentTools } from '../api';
import type { AgentToolDefinition } from '../api';
import { Icon } from './ui/Icon';

// The backend descriptions are prompt text aimed at the model, so they stay in
// English. These are the reader-facing equivalents; unknown tools fall back to
// whatever the backend sends.
const ZH: Record<string, { title: string; summary: string; params: Record<string, string> }> = {
  list_directory: {
    title: '列出目录',
    summary: '列出存储目录下某个路径的文件与子目录，用来摸清目录结构。',
    params: { path: '相对存储根目录的路径，留空或填 . 表示根目录' },
  },
  read_file: {
    title: '读取文件',
    summary: '读取文本文件的内容（超长会截断），在提出建议前先看清文件里有什么。',
    params: { path: '相对存储根目录的文件路径', limit: '最多读取的字符数，默认 2000' },
  },
  find_files: {
    title: '查找文件',
    summary: '按正则表达式递归匹配文件名，用于定位特定命名的文件。',
    params: { pattern: '针对文件名的正则表达式，例如 screenshot.*png' },
  },
  search_content: {
    title: '搜索内容',
    summary: '在文本文件内容中搜索关键词，返回命中的文件路径与所在行。',
    params: { pattern: '在文件内容中搜索的正则表达式', path: '可选，限定搜索的子目录' },
  },
  submit_plan: {
    title: '提交整理计划',
    summary: '提交一份文件整理计划等待你审批。在你批准之前不会执行任何操作。',
    params: {
      title: '计划标题',
      summary: '一句话说明这份计划要做什么',
      actions: '操作数组，每项描述一个移动、重命名或删除动作',
    },
  },
};

export function AgentToolsPage() {
  const [tools, setTools] = useState<AgentToolDefinition[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getAgentTools()
      .then(d => setTools(d.definitions || []))
      .catch(e => setError(e instanceof Error ? e.message : '加载失败'));
  }, []);

  return (
    <div className="h-full overflow-y-auto bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 flex items-center gap-2">
        <Icon name="code" size={20} className="text-indigo-600" />
        <span className="text-base font-semibold text-slate-800">Agent 能力</span>
      </div>

      <div className="max-w-3xl mx-auto p-4 sm:p-6 space-y-3">
        <p className="text-sm text-slate-500">
          Agent 只能通过下面这些工具读取你的文件。任何写入操作都只是生成待审批的计划，
          需要你在「整理」页确认后才会执行。
        </p>

        {error && (
          <div className="text-sm text-red-600 bg-red-50 rounded-lg px-3 py-2">加载失败：{error}</div>
        )}

        {tools === null && !error && (
          <div className="text-sm text-slate-400">加载中...</div>
        )}

        {tools?.map(t => {
          const name = t.function.name;
          const zh = ZH[name];
          const props = t.function.parameters?.properties || {};
          return (
            <section key={name} className="bg-white rounded-lg border border-slate-200 p-5">
              <div className="flex items-baseline gap-2 mb-1">
                <span className="text-base font-semibold text-slate-700">{zh?.title || name}</span>
                <code className="text-xs text-slate-400 font-mono">{name}</code>
              </div>
              <p className="text-sm text-slate-500 mb-3">{zh?.summary || t.function.description}</p>

              {Object.keys(props).length > 0 && (
                <div className="space-y-1.5">
                  {Object.entries(props).map(([param, spec]) => (
                    <div key={param} className="flex flex-wrap items-baseline gap-2 text-xs">
                      <code className="px-1.5 py-0.5 rounded bg-slate-100 text-slate-600 font-mono">{param}</code>
                      <span className="text-slate-400 font-mono">{spec.type}</span>
                      <span className="text-slate-500">
                        {zh?.params[param] || spec.description}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </section>
          );
        })}

        {tools?.length === 0 && (
          <div className="text-sm text-slate-400">Agent 当前没有可用的工具。</div>
        )}
      </div>
    </div>
  );
}