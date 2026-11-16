import { Fragment, useCallback, useEffect, useState } from 'react';
import { approvePlan, deletePlan, executePlan, fetchPlanLog, fetchPlans, rejectPlan } from '../../api';
import type { PlanLogEntry } from '../../api';
import { useToast } from '../ui/Toast';
import { ConfirmDialog } from '../ui/Dialog';
import { Icon } from '../ui/Icon';
import type { AgentPlan, PlanAction, PlanStatus } from '../../types';

const ACTION_META: Record<string, { icon: string; label: string; color: string }> = {
  move: { icon: 'move', label: '移动', color: 'bg-blue-50 text-blue-600' },
  rename: { icon: 'edit', label: '重命名', color: 'bg-violet-50 text-violet-600' },
  create_folder: { icon: 'folder', label: '新建文件夹', color: 'bg-green-50 text-green-600' },
  delete: { icon: 'trash', label: '删除', color: 'bg-red-50 text-red-600' },
  run: { icon: 'code', label: '执行命令', color: 'bg-orange-50 text-orange-600' },
};

const STATUS_META: Record<PlanStatus, { label: string; className: string }> = {
  pending: { label: '待审批', className: 'bg-amber-50 text-amber-600' },
  approved: { label: '已批准·待执行', className: 'bg-blue-50 text-blue-600' },
  rejected: { label: '已拒绝', className: 'bg-slate-100 text-slate-500' },
  executed: { label: '已执行', className: 'bg-green-50 text-green-600' },
  executed_with_errors: { label: '执行完成(部分失败)', className: 'bg-orange-50 text-orange-600' },
  failed: { label: '执行失败', className: 'bg-red-50 text-red-600' },
};

function ActionRow({ action }: { action: PlanAction }) {
  const meta = ACTION_META[action.action_type] ?? ACTION_META.move;
  // Every field carries a label. Two of them used to be bare text
  // flowing after the labelled ones -- the reason and the error had no
  // label at all, so a reader could not tell a long sentence apart
  // from a failure message. They were four lines of prose with the
  // first two prefixed and the last two not.
  const fields: { label: string; value: string; className: string }[] = [];
  if (action.command) fields.push({ label: '命令', value: action.command, className: 'font-mono text-slate-700' });
  if (action.cwd) fields.push({ label: '目录', value: action.cwd, className: 'text-slate-600' });
  if (action.source_path) fields.push({ label: '源', value: action.source_path, className: 'text-slate-600' });
  if (action.target_path) fields.push({ label: '目标', value: action.target_path, className: 'text-slate-600' });
  if (action.reason) fields.push({ label: '理由', value: action.reason, className: 'text-slate-400' });
  if (action.result && action.status === 'failed') fields.push({ label: '错误', value: action.result, className: 'text-red-500' });

  return (
    <div className="flex items-start gap-3 p-3 rounded-lg bg-slate-50">
      <div className={`w-7 h-7 shrink-0 rounded-md flex items-center justify-center ${meta.color}`}>
        <Icon name={meta.icon} size={15} />
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-xs font-medium text-slate-500">{meta.label}</span>
          {action.status === 'done' && <span className="text-xs text-green-600 flex items-center gap-0.5"><Icon name="check" size={12} />成功</span>}
          {action.status === 'failed' && <span className="text-xs text-red-600">失败</span>}
        </div>
        {fields.length > 0 && (
          <div className="mt-1 grid grid-cols-[2rem_1fr] gap-x-2 gap-y-1 items-start">
            {fields.map((f) => (
              <Fragment key={f.label}>
                <span className="text-xs text-slate-400 leading-4">{f.label}</span>
                <span
                  className={`text-xs leading-4 break-words ${f.className} ${f.label === '理由' ? 'line-clamp-2' : ''}`}
                  title={f.label === '理由' ? f.value : undefined}
                >{f.value}</span>
              </Fragment>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/** When each thing was actually tried, and what happened. */
function ExecutionLog({ planId }: { planId: string }) {
  const [entries, setEntries] = useState<PlanLogEntry[] | null>(null);
  useEffect(() => {
    let alive = true;
    fetchPlanLog(planId)
      .then((rows) => { if (alive) setEntries(rows); })
      .catch(() => { if (alive) setEntries([]); });
    return () => { alive = false; };
  }, [planId]);

  if (!entries) {
    return <div className="text-xs text-slate-400 px-3 py-2">正在读执行记录…</div>;
  }
  if (entries.length === 0) {
    return <div className="text-xs text-slate-400 px-3 py-2">还没有执行记录</div>;
  }
  return (
    <div className="px-3 py-2 space-y-1">
      {entries.map((e) => (
        <div key={e.id} className="flex items-start gap-2 text-xs">
          <span className={e.success ? 'text-green-600' : 'text-red-600'}>
            {e.success ? '成功' : '失败'}
          </span>
          <span className="text-slate-400 tabular-nums flex-shrink-0">
            {new Date(e.timestamp).toLocaleString('zh-CN', { hour12: false })}
          </span>
          <span className="text-slate-600 break-words">
            {e.action_type}
            {e.source_path ? ` · ${e.source_path}` : ''}
            {e.target_path ? ` → ${e.target_path}` : ''}
            {e.message ? ` · ${e.message}` : ''}
          </span>
        </div>
      ))}
    </div>
  );
}

function PlanCard({ plan, onChanged }: { plan: AgentPlan; onChanged: () => void }) {
  const { toast } = useToast();
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const statusMeta = STATUS_META[plan.status] ?? STATUS_META.pending;
  const isPending = plan.status === 'pending';

  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try { await fn(); onChanged(); } catch (e) { toast(e instanceof Error ? e.message : '操作失败', 'error'); } finally { setBusy(false); }
  };

  const handleApproveAndExecute = () => {
    setConfirming(true);
  };

  const handleConfirmExecute = async () => {
    setConfirming(false);
    setBusy(true);
    try {
      await approvePlan(plan.id);
      await executePlan(plan.id);
      onChanged();
    } catch (e) { toast(e instanceof Error ? e.message : '操作失败', 'error'); } finally { setBusy(false); }
  };

  return (
    <div className="bg-white rounded-lg border border-slate-200 overflow-hidden">
      <div className="p-4 flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className={`px-2 py-0.5 rounded-full text-xs font-medium ${statusMeta.className}`}>{statusMeta.label}</span>
            <span className="text-sm font-semibold text-slate-800">{plan.title}</span>
          </div>
          {plan.summary && <div className="text-xs text-slate-500 mt-1 line-clamp-2">{plan.summary}</div>}
          <div className="text-xs text-slate-400 mt-1">{plan.actions.length} 个操作 · 来源: {plan.source === 'agent' ? 'Agent' : '手动'}{plan.executed_at && ` · ${new Date(plan.executed_at).toLocaleString('zh-CN')}`}</div>
        </div>
        <div className="flex flex-col gap-1.5 shrink-0">
          {isPending && (
            <>
              <button onClick={handleApproveAndExecute} disabled={busy}
                className="flex items-center gap-1 px-3 py-1.5 rounded-lg bg-green-600 text-white text-xs font-medium hover:bg-green-700 disabled:opacity-50 transition-colors">
                <Icon name="check" size={14} />批准并执行
              </button>
              <button onClick={() => run(() => rejectPlan(plan.id))} disabled={busy}
                className="px-3 py-1.5 rounded-lg bg-slate-100 text-slate-600 text-xs font-medium hover:bg-slate-200 disabled:opacity-50 transition-colors">拒绝</button>
            </>
          )}
          {!isPending && (
            <button onClick={() => run(() => deletePlan(plan.id))} disabled={busy}
              className="px-3 py-1.5 rounded-lg bg-slate-100 text-slate-500 text-xs font-medium hover:bg-slate-200 disabled:opacity-50 transition-colors">清除</button>
          )}
        </div>
      </div>
      {expanded && (
        <div className="px-4 pb-3 space-y-2">
          {plan.actions.map(a => <ActionRow key={a.id} action={a} />)}
          {/* The actions above say what the plan wanted to do; the log
              says when each of them was actually tried, and what came
              of it. A plan that was run twice has two stories, and only
              the log keeps both. */}
          <details className="text-xs">
            <summary className="cursor-pointer text-slate-400 hover:text-slate-500 select-none">
              执行记录
            </summary>
            <ExecutionLog planId={plan.id} />
          </details>
        </div>
      )}
      {/* The toggle sits below the details it controls. Above them it read
          as a heading for the plan rather than a control over the list. */}
      <button onClick={() => setExpanded(v => !v)}
        className="w-full px-4 pb-3 text-left text-xs text-indigo-500 hover:text-indigo-600 transition-colors flex items-center gap-1">
        <Icon name={expanded ? 'chevronDown' : 'chevronRight'} size={14} />
        {expanded ? '收起操作详情' : `展开操作详情 (${plan.actions.length})`}
      </button>
      <ConfirmDialog
        open={confirming}
        title="执行计划"
        message={`确认执行计划「${plan.title}」？该操作将真实修改文件系统。`}
        confirmText="执行"
        danger
        onConfirm={handleConfirmExecute}
        onCancel={() => setConfirming(false)}
      />
    </div>
  );
}

export function ApprovalCenter() {
  const [plans, setPlans] = useState<AgentPlan[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [filter, setFilter] = useState<'pending' | 'all'>('pending');

  const load = useCallback(async () => {
    setIsLoading(true);
    try { setPlans(await fetchPlans(filter === 'pending' ? 'pending' : undefined)); }
    catch (e) { console.error('Failed:', e); } finally { setIsLoading(false); }
  }, [filter]);

  useEffect(() => { load(); const timer = setInterval(load, 10000); return () => clearInterval(timer); }, [load]);

  return (
    <div className="bg-slate-50 rounded-lg border border-slate-200 p-4">
      <div className="flex items-center justify-between gap-2 mb-3">
        <div className="flex items-center gap-2 min-w-0">
          <Icon name="shield" size={18} className="text-indigo-500 flex-shrink-0" />
          <span className="text-base font-semibold text-slate-700 whitespace-nowrap">审批中心</span>
          {/* The title already says what this is. The sentence beside it
              took 12 characters of the header on a phone and pushed the
              whole row onto two lines; there is nothing to say here that
              the heading does not. */}
          <span className="text-xs text-slate-400 hidden lg:inline">所有文件变更必须经人工批准</span>
        </div>
        <div className="flex gap-1 flex-shrink-0">
          <button onClick={() => setFilter('pending')}
            className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${filter === 'pending' ? 'bg-indigo-600 text-white' : 'bg-white text-slate-600 hover:bg-slate-100'}`}>待审批</button>
          <button onClick={() => setFilter('all')}
            className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${filter === 'all' ? 'bg-indigo-600 text-white' : 'bg-white text-slate-600 hover:bg-slate-100'}`}>全部</button>
        </div>
      </div>
      {isLoading ? (
        <div className="text-center py-6 text-slate-400 text-sm">加载中...</div>
      ) : plans.length === 0 ? (
        <div className="text-center py-6 text-slate-400">
          <Icon name="checkCircle" size={32} className="mx-auto text-slate-300 mb-2" />
          <div className="text-sm">{filter === 'pending' ? '没有待审批的计划' : '暂无计划记录'}</div>
          <div className="text-xs mt-1">在「Agent 对话」中让 AI 整理文件</div>
        </div>
      ) : (
        <div className="space-y-2">{plans.map(plan => <PlanCard key={plan.id} plan={plan} onChanged={load} />)}</div>
      )}
    </div>
  );
}
