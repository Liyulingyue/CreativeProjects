import { useState, useEffect, useCallback } from 'react';
import { api } from './api';
import type { NodeInfo, MasterStatus, NodeStatus } from './types';
import './App.css';

type Tab = 'nodes' | 'status' | 'experts' | 'inference' | 'logs';

function App() {
  const [tab, setTab] = useState<Tab>('nodes');
  const [nodes, setNodes] = useState<NodeInfo[]>([]);
  const [toast, setToast] = useState<{ msg: string; type: string } | null>(null);
  const [loading, setLoading] = useState(false);

  const showToast = useCallback((msg: string, type = 'success') => {
    setToast({ msg, type });
    setTimeout(() => setToast(null), 3000);
  }, []);

  const refreshNodes = useCallback(async () => {
    try {
      setNodes(await api.getNodes());
    } catch (e) {
      // ignore
    }
  }, []);

  useEffect(() => {
    refreshNodes();
    const interval = setInterval(refreshNodes, 5000);
    return () => clearInterval(interval);
  }, [refreshNodes]);

  const masters = nodes.filter(n => n.node_type === 'master' && n.alive);

  return (
    <div className="app">
      {toast && (
        <div className={`toast ${toast.type}`}>{toast.msg}</div>
      )}

      <header className="app-header">
        <h1>MoEAllocator</h1>
        <span className="subtitle">分布式 MoE 推理框架</span>
      </header>

      <nav className="tabs">
        {(['nodes', 'status', 'experts', 'inference', 'logs'] as Tab[]).map(t => (
          <button key={t} className={`tab ${tab === t ? 'active' : ''}`} onClick={() => setTab(t)}>
            {t === 'nodes' ? '节点管理' : t === 'status' ? '状态监控' : t === 'experts' ? 'Expert 管理' : t === 'inference' ? '推理' : '日志'}
          </button>
        ))}
      </nav>

      <main className="content">
        {tab === 'nodes' && (
          <NodesTab nodes={nodes} masters={masters} onRefresh={refreshNodes} showToast={showToast} />
        )}
        {tab === 'status' && (
          <StatusTab nodes={nodes} />
        )}
        {tab === 'experts' && (
          <ExpertsTab masters={masters} showToast={showToast} />
        )}
        {tab === 'inference' && (
          <InferenceTab masters={masters} showToast={showToast} loading={loading} setLoading={setLoading} />
        )}
        {tab === 'logs' && (
          <LogsTab nodes={nodes} />
        )}
      </main>
    </div>
  );
}

function NodesTab({ nodes, masters, onRefresh, showToast }: {
  nodes: NodeInfo[]; masters: NodeInfo[]; onRefresh: () => void; showToast: (m: string, t?: string) => void;
}) {
  const [deleting, setDeleting] = useState<string | null>(null);

  const deleteNode = async (nodeId: string) => {
    if (!confirm(`确认删除节点 ${nodeId}？`)) return;
    setDeleting(nodeId);
    try {
      await api.deleteNode(nodeId);
      showToast(`节点 ${nodeId} 已删除`);
      onRefresh();
    } catch (e: unknown) {
      showToast((e as Error).message, 'error');
    } finally {
      setDeleting(null);
    }
  };

  return (
    <div className="tab-content">
      <div className="card-grid">
        <div className="card">
          <h2>创建 Master</h2>
          <MasterForm onDone={() => { onRefresh(); showToast('Master 已启动'); }} showToast={showToast} />
        </div>
        <div className="card">
          <h2>创建 Worker</h2>
          <WorkerForm masters={masters} onDone={() => { onRefresh(); showToast('Worker 已启动'); }} showToast={showToast} />
        </div>
      </div>

      <div className="card">
        <h2>节点列表</h2>
        <div className="nodes-list">
          {nodes.length === 0 && <p className="empty">暂无节点</p>}
          {nodes.map(n => (
            <div key={n.node_id} className="node-row">
              <div className={`dot ${n.alive ? 'alive' : 'dead'}`} />
              <span className={`badge ${n.node_type}`}>{n.node_type.toUpperCase()}</span>
              <span className="node-id">{n.node_id}</span>
              <span className="node-info">HTTP:{n.http_port} {n.tcp_port ? `TCP:${n.tcp_port}` : ''} PID:{n.pid}</span>
              <button className="btn-danger btn-sm" onClick={() => deleteNode(n.node_id)} disabled={deleting === n.node_id}>
                {deleting === n.node_id ? '删除中...' : '删除'}
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function MasterForm({ onDone, showToast }: {
  onDone: () => void; showToast: (m: string, t?: string) => void;
}) {
  const [nodeId, setNodeId] = useState('master-1');
  const [manifest, setManifest] = useState('output/splits/ERNIE-4.5-21B-A3B-PT-k6/manifest.json');
  const [port, setPort] = useState('');
  const [experts, setExperts] = useState('0,1,2');
  const [loading, setLoading] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!nodeId || !manifest) return showToast('请填写节点ID和Manifest路径', 'error');
    setLoading(true);
    try {
      await api.createMaster({
        node_id: nodeId,
        manifest_path: manifest,
        http_port: port ? parseInt(port) : undefined,
        expert_ids: experts ? experts.split(',').map(Number) : undefined,
      });
      onDone();
    } catch (err: unknown) {
      showToast((err as Error).message, 'error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <form onSubmit={submit} className="form-grid">
      <div className="form-group">
        <label>节点 ID</label>
        <input value={nodeId} onChange={e => setNodeId(e.target.value)} />
      </div>
      <div className="form-group">
        <label>Manifest 路径</label>
        <input value={manifest} onChange={e => setManifest(e.target.value)} />
      </div>
      <div className="form-group">
        <label>HTTP 端口（留空自动分配）</label>
        <input type="number" value={port} onChange={e => setPort(e.target.value)} />
      </div>
      <div className="form-group">
        <label>Expert IDs（逗号分隔）</label>
        <input value={experts} onChange={e => setExperts(e.target.value)} />
      </div>
      <button type="submit" className="btn-primary" disabled={loading} style={{ gridColumn: '1/-1' }}>
        {loading ? '启动中...' : '启动 Master'}
      </button>
    </form>
  );
}

function WorkerForm({ masters, onDone, showToast }: {
  masters: NodeInfo[]; onDone: () => void; showToast: (m: string, t?: string) => void;
}) {
  const [nodeId, setNodeId] = useState('worker-1');
  const [expertsDir, setExpertsDir] = useState('output/splits/ERNIE-4.5-21B-A3B-PT-k6');
  const [expertIds, setExpertIds] = useState('3,4,5');
  const [masterId, setMasterId] = useState(masters[0]?.node_id || '');
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (masters.length > 0 && !masters.find(m => m.node_id === masterId)) {
      setMasterId(masters[0].node_id);
    }
  }, [masters, masterId]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!nodeId) return showToast('请填写节点ID', 'error');
    setLoading(true);
    try {
      const master = masters.find(m => m.node_id === masterId);
      await api.createWorker({
        node_id: nodeId,
        experts_dir: expertsDir || undefined,
        expert_ids: expertIds ? expertIds.split(',').map(Number) : undefined,
        master_id: master ? masterId : undefined,
        master_url: master ? `http://localhost:${master.http_port}` : undefined,
      });
      onDone();
    } catch (err: unknown) {
      showToast((err as Error).message, 'error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <form onSubmit={submit} className="form-grid">
      <div className="form-group">
        <label>节点 ID</label>
        <input value={nodeId} onChange={e => setNodeId(e.target.value)} />
      </div>
      <div className="form-group">
        <label>Experts 目录</label>
        <input value={expertsDir} onChange={e => setExpertsDir(e.target.value)} />
      </div>
      <div className="form-group">
        <label>Expert IDs（逗号分隔）</label>
        <input value={expertIds} onChange={e => setExpertIds(e.target.value)} />
      </div>
      <div className="form-group">
        <label>注册到 Master</label>
        <select value={masterId} onChange={e => setMasterId(e.target.value)}>
          <option value="">-- 不注册 --</option>
          {masters.map(m => (
            <option key={m.node_id} value={m.node_id}>{m.node_id} (HTTP {m.http_port})</option>
          ))}
        </select>
      </div>
      <button type="submit" className="btn-primary" disabled={loading} style={{ gridColumn: '1/-1' }}>
        {loading ? '启动中...' : '启动 Worker'}
      </button>
    </form>
  );
}

function StatusTab({ nodes }: { nodes: NodeInfo[] }) {
  const [selected, setSelected] = useState(nodes.find(n => n.alive)?.node_id || '');
  const [status, setStatus] = useState<NodeStatus | null>(null);

  useEffect(() => {
    if (!selected) return;
    api.getNodeStatus(selected).then(setStatus).catch(() => setStatus(null));
  }, [selected]);

  useEffect(() => {
    if (nodes.find(n => n.node_id === selected && n.alive)) {
      api.getNodeStatus(selected).then(setStatus).catch(() => setStatus(null));
    }
  }, [nodes, selected]);

  const selectedNode = nodes.find(n => n.node_id === selected);
  const isMaster = status && 'local_expert_count' in status;

  return (
    <div className="tab-content">
      <div className="card">
        <h2>节点状态</h2>
        <div className="form-group" style={{ marginBottom: 16 }}>
          <select value={selected} onChange={e => setSelected(e.target.value)}>
            <option value="">-- 选择节点 --</option>
            {nodes.filter(n => n.alive).map(n => (
              <option key={n.node_id} value={n.node_id}>{n.node_id} ({n.node_type}, HTTP {n.http_port})</option>
            ))}
          </select>
        </div>

        {selectedNode && status && (
          <div className="status-grid">
            <div className="status-item">
              <span className="status-label">类型</span>
              <span className="status-value">{isMaster ? 'MASTER' : 'WORKER'}</span>
            </div>
            <div className="status-item">
              <span className="status-label">状态</span>
              <span className={`status-value ${selectedNode.alive ? 'good' : 'bad'}`}>
                {selectedNode.alive ? '运行中' : '已停止'}
              </span>
            </div>
            <div className="status-item">
              <span className="status-label">HTTP 端口</span>
              <span className="status-value">{selectedNode.http_port}</span>
            </div>
            {selectedNode.tcp_port && (
              <div className="status-item">
                <span className="status-label">TCP 端口</span>
                <span className="status-value">{selectedNode.tcp_port}</span>
              </div>
            )}
            <div className="status-item">
              <span className="status-label">本地 Experts</span>
              <span className="status-value">{isMaster ? (status as MasterStatus).local_expert_count : (status as { loaded_count: number }).loaded_count}</span>
            </div>
            {isMaster && (
              <div className="status-item">
                <span className="status-label">Workers</span>
                <span className="status-value">{(status as MasterStatus).workers}</span>
              </div>
            )}
          </div>
        )}

        {selectedNode && status && (
          <div style={{ marginTop: 16 }}>
            <div style={{ fontSize: 12, color: '#9ca3af', marginBottom: 8 }}>
              Experts 列表
            </div>
            <div className="experts-list">
              {isMaster
                ? (status as MasterStatus).local_experts.map((e: string, i: number) => (
                  <span key={i} className="expert-chip">{e}</span>
                ))
                : (status as { loaded_experts: string[] }).loaded_experts.map((e: string, i: number) => (
                  <span key={i} className="expert-chip">{e}</span>
                ))
              }
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ExpertsTab({ masters, showToast }: { masters: NodeInfo[]; showToast: (m: string, t?: string) => void }) {
  const [selected, setSelected] = useState(masters[0]?.node_id || '');
  const [expertId, setExpertId] = useState('');
  const [layerId, setLayerId] = useState('');
  const [experts, setExperts] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (masters.length > 0 && !masters.find(m => m.node_id === selected)) {
      setSelected(masters[0].node_id);
    }
  }, [masters, selected]);

  const refreshExperts = useCallback(async () => {
    if (!selected) return;
    try {
      const s = await api.getNodeStatus(selected) as MasterStatus;
      setExperts(s.local_experts || []);
    } catch {
      setExperts([]);
    }
  }, [selected]);

  useEffect(() => {
    refreshExperts();
    const interval = setInterval(refreshExperts, 3000);
    return () => clearInterval(interval);
  }, [refreshExperts]);

  const loadExpert = async () => {
    if (!selected) return showToast('请选择 Master 节点', 'error');
    if (!expertId || !layerId) return showToast('请填写 expert_id 和 layer_id', 'error');
    setLoading(true);
    try {
      const res = await api.loadExpert({ node_id: selected, expert_id: parseInt(expertId), layer_id: parseInt(layerId) });
      showToast(`Expert expert_${expertId}_layer_${layerId} 已加载 (${res.size_mb.toFixed(1)} MB)`);
      setExperts(res.local_experts || []);
    } catch (err: unknown) {
      showToast((err as Error).message, 'error');
    } finally {
      setLoading(false);
    }
  };

  const unloadExpert = async () => {
    if (!selected) return showToast('请选择 Master 节点', 'error');
    if (!expertId || !layerId) return showToast('请填写 expert_id 和 layer_id', 'error');
    setLoading(true);
    try {
      await api.unloadExpert({ node_id: selected, expert_id: parseInt(expertId), layer_id: parseInt(layerId) });
      showToast(`Expert expert_${expertId}_layer_${layerId} 已卸载`);
      refreshExperts();
    } catch (err: unknown) {
      showToast((err as Error).message, 'error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="tab-content">
      <div className="card">
        <h2>运行时加载 / 卸载 Expert</h2>
        <div className="form-grid" style={{ marginBottom: 16 }}>
          <div className="form-group">
            <label>选择 Master 节点</label>
            <select value={selected} onChange={e => setSelected(e.target.value)}>
              <option value="">-- 选择 Master --</option>
              {masters.map(m => (
                <option key={m.node_id} value={m.node_id}>{m.node_id} (HTTP {m.http_port})</option>
              ))}
            </select>
          </div>
          <div className="form-group">
            <label>Expert ID (0-63)</label>
            <input type="number" value={expertId} onChange={e => setExpertId(e.target.value)} placeholder="如: 6" min="0" max="63" />
          </div>
          <div className="form-group">
            <label>Layer ID (1-27)</label>
            <input type="number" value={layerId} onChange={e => setLayerId(e.target.value)} placeholder="如: 1" min="1" max="27" />
          </div>
        </div>
        <div style={{ display: 'flex', gap: 12 }}>
          <button className="btn-primary" onClick={loadExpert} disabled={loading}>加载 Expert</button>
          <button className="btn-danger" onClick={unloadExpert} disabled={loading}>卸载 Expert</button>
        </div>

        <div style={{ marginTop: 20 }}>
          <div style={{ fontSize: 12, color: '#9ca3af', marginBottom: 8 }}>
            当前已加载的 Experts（{experts.length} 个）
          </div>
          <div className="experts-list">
            {experts.length === 0 && <span style={{ color: '#6b7280' }}>当前无本地 experts</span>}
            {experts.map((e, i) => (
              <span key={i} className="expert-chip">{e}</span>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function InferenceTab({ masters, showToast, loading, setLoading }: {
  masters: NodeInfo[]; showToast: (m: string, t?: string) => void; loading: boolean; setLoading: (v: boolean) => void;
}) {
  const [masterId, setMasterId] = useState(masters[0]?.node_id || '');
  const [prompt, setPrompt] = useState('今天天气真好，');
  const [maxTokens, setMaxTokens] = useState('20');
  const [result, setResult] = useState('');
  const [inputTokens, setInputTokens] = useState(0);
  const [outputTokens, setOutputTokens] = useState(0);

  useEffect(() => {
    if (masters.length > 0 && !masters.find(m => m.node_id === masterId)) {
      setMasterId(masters[0].node_id);
    }
  }, [masters, masterId]);

  const run = async () => {
    if (!masterId) return showToast('请选择 Master', 'error');
    if (!prompt.trim()) return showToast('请输入 prompt', 'error');
    setLoading(true);
    setResult('');
    try {
      const res = await api.inference({
        master_node_id: masterId,
        prompt,
        max_tokens: parseInt(maxTokens) || 20,
      });
      setResult(res.result || '(无输出)');
      setInputTokens(res.input_tokens);
      setOutputTokens(res.output_tokens);
      showToast('推理完成');
    } catch (err: unknown) {
      showToast((err as Error).message, 'error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="tab-content">
      <div className="card">
        <h2>推理</h2>
        <div className="form-grid" style={{ marginBottom: 12 }}>
          <div className="form-group">
            <label>选择 Master</label>
            <select value={masterId} onChange={e => setMasterId(e.target.value)}>
              <option value="">-- 选择 Master --</option>
              {masters.map(m => (
                <option key={m.node_id} value={m.node_id}>{m.node_id} (HTTP {m.http_port})</option>
              ))}
            </select>
          </div>
          <div className="form-group">
            <label>Max Tokens</label>
            <input type="number" value={maxTokens} onChange={e => setMaxTokens(e.target.value)} min="1" max="1024" />
          </div>
        </div>
        <textarea
          className="prompt-area"
          value={prompt}
          onChange={e => setPrompt(e.target.value)}
          placeholder="输入 prompt..."
        />
        <button className="btn-primary" onClick={run} disabled={loading} style={{ marginTop: 12 }}>
          {loading ? '推理中...' : '推理'}
        </button>

        {result && (
          <>
            <div className="result-box">{result}</div>
            <div className="meta-row">
              <span>Input tokens: {inputTokens}</span>
              <span>Output tokens: {outputTokens}</span>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function LogsTab({ nodes }: { nodes: NodeInfo[] }) {
  const [selected, setSelected] = useState('');
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!selected) return;
    setLoading(true);
    api.getNodeLogs(selected)
      .then(res => setContent(res.content || '(无可用日志)'))
      .catch(err => setContent('日志加载失败: ' + err.message))
      .finally(() => setLoading(false));
  }, [selected]);

  return (
    <div className="tab-content">
      <div className="card">
        <h2>节点日志</h2>
        <div className="form-group" style={{ marginBottom: 16 }}>
          <select value={selected} onChange={e => setSelected(e.target.value)}>
            <option value="">-- 选择节点 --</option>
            {nodes.map(n => (
              <option key={n.node_id} value={n.node_id}>{n.node_id} ({n.node_type}, HTTP {n.http_port})</option>
            ))}
          </select>
        </div>
        <div className="log-area">
          {loading ? '加载中...' : content}
        </div>
      </div>
    </div>
  );
}

export default App;
