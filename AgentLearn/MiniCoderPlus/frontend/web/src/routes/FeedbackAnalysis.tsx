import React, { useState, useEffect } from 'react';
import { Download, Search, ChevronLeft, ChevronRight, Trash2 } from 'lucide-react';
import AppHeader from '../components/AppHeader';
import Modal from '../components/Modal';
import '../styles/FeedbackAnalysis.css';

interface Feedback {
  id: number;
  message_id: number;
  session_id: string;
  feedback: string;
  comment?: string;
  context_snapshot?: string;
  timestamp: string;
  message_content: string;
  role: string;
}

interface FeedbackResponse {
  items: Feedback[];
  total: number;
  limit: number;
  offset: number;
  filters: {
    feedback_type?: string;
    session_id?: string;
  };
}

const THUMBS_UP = '👍';
const THUMBS_DOWN = '👎';

const FeedbackAnalysis: React.FC = () => {
  const [feedbacks, setFeedbacks] = useState<Feedback[]>([]);
  const [loading, setLoading] = useState(false);
  const [total, setTotal] = useState(0);

  // Selection state
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [selectedContext, setSelectedContext] = useState<any[] | null>(null);

  // Modal state
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  // Filter states
  const [feedbackType, setFeedbackType] = useState<'all' | '👍' | '👎'>('all');
  const [sessionFilter, setSessionFilter] = useState('');
  const [searchText, setSearchText] = useState('');

  // Pagination
  const [page, setPage] = useState(1);
  const pageSize = 20;

  // Sort
  const [sortBy, setSortBy] = useState<'timestamp DESC' | 'timestamp ASC'>('timestamp DESC');

  const fetchFeedbacks = async () => {
    setLoading(true);
    try {
      const feedbackType_ = feedbackType === 'all' ? undefined : feedbackType;
      const sessionId_ = sessionFilter.trim() || undefined;
      const offset = (page - 1) * pageSize;

      const params = new URLSearchParams();
      if (feedbackType_) params.append('feedback_type', feedbackType_);
      if (sessionId_) params.append('session_id', sessionId_);
      params.append('limit', pageSize.toString());
      params.append('offset', offset.toString());
      params.append('order_by', sortBy);

      const resp = await fetch(`/api/v1/chat/feedbacks?${params}`);
      if (!resp.ok) throw new Error('Failed to load feedbacks');

      const data: FeedbackResponse = await resp.json();
      
      // Filter by search text if provided
      let filtered = data.items;
      if (searchText.trim()) {
        const q = searchText.toLowerCase();
        filtered = filtered.filter(f => 
          f.message_content.toLowerCase().includes(q) ||
          f.comment?.toLowerCase().includes(q) ||
          f.session_id.toLowerCase().includes(q)
        );
      }

      setFeedbacks(filtered);
      setTotal(data.total);
      setSelectedIds(new Set()); // Clear selection on new data
    } catch (error) {
      console.error('Failed to load feedbacks:', error);
      setFeedbacks([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setPage(1);
  }, [feedbackType, sessionFilter, sortBy]);

  useEffect(() => {
    fetchFeedbacks();
  }, [page, feedbackType, sessionFilter, sortBy]);

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    setPage(1);
    fetchFeedbacks();
  };

  const toggleSelect = (id: number) => {
    const newSelected = new Set(selectedIds);
    if (newSelected.has(id)) {
      newSelected.delete(id);
    } else {
      newSelected.add(id);
    }
    setSelectedIds(newSelected);
  };

  const toggleSelectAll = () => {
    if (selectedIds.size === feedbacks.length && feedbacks.length > 0) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(feedbacks.map(f => f.id)));
    }
  };

  const handleBatchDelete = async () => {
    if (selectedIds.size === 0) return;
    setIsDeleting(true);
    try {
      const resp = await fetch(`/api/v1/chat/feedbacks/batch`, {
        method: 'DELETE',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(Array.from(selectedIds)),
      });
      
      if (!resp.ok) {
        throw new Error('删除失败');
      }

      // Refresh data
      setIsDeleteModalOpen(false);
      fetchFeedbacks();
    } catch (error) {
      console.error('Failed to delete feedbacks:', error);
      alert('删除失败，请稍后重试');
    } finally {
      setIsDeleting(false);
    }
  };

  const exportCSV = () => {
    // Determine which data to export: selected items or all currently loaded items
    const dataToExport = selectedIds.size > 0 
      ? feedbacks.filter(f => selectedIds.has(f.id))
      : feedbacks;

    if (dataToExport.length === 0) {
      alert('没有可导出的反馈记录');
      return;
    }

    const headers = ['ID', 'Session ID', 'Feedback', 'Comment', 'Message Content', 'Context', 'Timestamp'];
    const rows = dataToExport.map(f => {
      // Map emoji to text for better CSV compatibility
      const feedbackText = f.feedback === THUMBS_UP ? 'good' : (f.feedback === THUMBS_DOWN ? 'bad' : f.feedback);
      
      return [
        f.id,
        f.session_id,
        feedbackText,
        `"${(f.comment || '').replace(/"/g, '""')}"`,
        `"${(f.message_content || '').replace(/"/g, '""')}"`,
        `"${(f.context_snapshot || '').replace(/"/g, '""')}"`,
        f.timestamp
      ];
    });

    const csv = [
      headers.join(','),
      ...rows.map(r => r.join(','))
    ].join('\n');

    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const link = document.createElement('a');
    const url = URL.createObjectURL(blob);
    const filename = selectedIds.size > 0 
      ? `feedbacks_selected_${new Date().toISOString().split('T')[0]}.csv`
      : `feedbacks_all_${new Date().toISOString().split('T')[0]}.csv`;
    
    link.setAttribute('href', url);
    link.setAttribute('download', filename);
    link.style.visibility = 'hidden';
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
  };

  const totalPages = Math.ceil(total / pageSize);

  const getPageRange = () => {
    const delta = 2;
    const range = [];
    const rangeWithDots: (number | string)[] = [];
    let l;

    for (let i = 1; i <= totalPages; i++) {
      if (i === 1 || i === totalPages || (i >= page - delta && i <= page + delta)) {
        range.push(i);
      }
    }

    for (const i of range) {
      if (l) {
        if (i - l === 2) {
          rangeWithDots.push(l + 1);
        } else if (i - l !== 1) {
          rangeWithDots.push('...');
        }
      }
      rangeWithDots.push(i);
      l = i;
    }

    return rangeWithDots;
  };

  return (
    <div className="mini-coder-app">
      <AppHeader
        title="📊 反馈分析"
        links={[
          { to: '/', label: '🏠 首页' },
          { to: '/workbench', label: '🛠️ 工作台' },
          { to: '/feedback-analysis', label: '📊 反馈分析' }
        ]}
        workspace={undefined}
      />

      <div className="feedback-analysis-container">
      {/* Filters */}
      <div className="filters-panel">
        <div className="filter-group">
          <label className="filter-label">评分筛选：</label>
          <select 
            value={feedbackType}
            onChange={(e) => setFeedbackType(e.target.value as any)}
            className="filter-select"
          >
            <option value="all">全部</option>
            <option value="👍">好评 👍</option>
            <option value="👎">差评 👎</option>
          </select>
        </div>

        <div className="filter-group">
          <label className="filter-label">排序：</label>
          <select 
            value={sortBy}
            onChange={(e) => setSortBy(e.target.value as any)}
            className="filter-select"
          >
            <option value="timestamp DESC">最新优先</option>
            <option value="timestamp ASC">最旧优先</option>
          </select>
        </div>

        <form onSubmit={handleSearch} className="search-form">
          <input
            type="text"
            placeholder="按会话 ID 或关键词搜索..."
            value={sessionFilter || searchText}
            onChange={(e) => {
              setSessionFilter(e.target.value);
              setSearchText(e.target.value);
            }}
            className="search-input"
          />
          <button type="submit" className="search-btn" title="搜索">
            <Search size={16} />
            搜索
          </button>
        </form>

        <button 
          onClick={() => setIsDeleteModalOpen(true)} 
          className="batch-delete-btn" 
          disabled={selectedIds.size === 0}
          title="删除选中记录"
        >
          <Trash2 size={16} />
          批量删除 ({selectedIds.size})
        </button>

        <button onClick={exportCSV} className="export-btn" title="导出CSV用于模型训练">
          <Download size={16} />
          导出 CSV
        </button>
      </div>

      {/* Stats */}
      <div className="stats-bar">
        <span className="stat-item">
          总数：<strong>{total}</strong>
        </span>
        {feedbackType === '👍' && (
          <span className="stat-item" style={{ color: '#059669' }}>
            👍 好评
          </span>
        )}
        {feedbackType === '👎' && (
          <span className="stat-item" style={{ color: '#ef4444' }}>
            👎 差评
          </span>
        )}
        {feedbacks.length > 0 && (
          <span className="stat-item">
            当前页：{feedbacks.length} / 共 {totalPages} 页
          </span>
        )}
      </div>

      {/* Feedbacks Table */}
      <div className="feedback-table-wrapper">
        {loading ? (
          <div className="loading-state">加载中...</div>
        ) : feedbacks.length === 0 ? (
          <div className="empty-state">
            <p>暂无反馈数据</p>
          </div>
        ) : (
          <table className="feedback-table">
            <thead>
              <tr>
                <th style={{ width: '40px' }}>
                  <input 
                    type="checkbox" 
                    checked={feedbacks.length > 0 && selectedIds.size === feedbacks.length}
                    onChange={toggleSelectAll}
                  />
                </th>
                <th style={{ width: '60px' }}>ID</th>
                <th style={{ width: '80px' }}>评分</th>
                <th style={{ width: '100px' }}>会话 ID</th>
                <th>原始消息预览</th>
                <th>用户评论</th>
                <th style={{ width: '100px' }}>上下文</th>
                <th style={{ width: '160px' }}>时间</th>
              </tr>
            </thead>
            <tbody>
              {feedbacks.map((f) => (
                <tr key={f.id} className={`feedback-row ${selectedIds.has(f.id) ? 'selected' : ''}`}>
                  <td>
                    <input 
                      type="checkbox" 
                      checked={selectedIds.has(f.id)}
                      onChange={() => toggleSelect(f.id)}
                    />
                  </td>
                  <td className="id-cell">{f.id}</td>
                  <td>
                    <span className={`feedback-badge ${f.feedback === THUMBS_UP ? 'positive' : 'negative'}`}>
                      {f.feedback}
                    </span>
                  </td>
                  <td className="session-id-cell">
                    <code className="session-id-text" title={f.session_id}>{f.session_id.substring(0, 8)}...</code>
                  </td>
                  <td className="content-cell">
                    <div className="message-content-preview" title={f.message_content}>
                      {f.message_content}
                    </div>
                  </td>
                  <td className="comment-cell">
                    {f.comment ? (
                      <div className="comment-text-preview" title={f.comment}>
                        {f.comment}
                      </div>
                    ) : (
                      <span className="no-comment">-</span>
                    )}
                  </td>
                  <td className="context-action-cell">
                    {f.context_snapshot ? (
                      <button 
                        className="view-context-btn"
                        onClick={() => {
                          try {
                            setSelectedContext(JSON.parse(f.context_snapshot || '[]'));
                          } catch (e) {
                            console.error('Failed to parse context', e);
                          }
                        }}
                        title="查看对话快照"
                      >
                        查看
                      </button>
                    ) : (
                      <span className="no-context">-</span>
                    )}
                  </td>
                  <td className="timestamp-cell">
                    {new Date(f.timestamp).toLocaleString('zh-CN')}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="pagination">
          <button 
            onClick={() => setPage(Math.max(1, page - 1))}
            disabled={page === 1}
            className="pagination-btn"
            title="上一页"
          >
            <ChevronLeft size={16} />
          </button>
          
          <div className="pagination-pages">
            {getPageRange().map((p, idx) => (
              p === '...' ? (
                <span key={`dots-${idx}`} className="pagination-dots">...</span>
              ) : (
                <button
                  key={`page-${p}`}
                  onClick={() => setPage(p as number)}
                  className={`pagination-page-btn ${page === p ? 'active' : ''}`}
                >
                  {p}
                </button>
              )
            ))}
          </div>

          <button 
            onClick={() => setPage(Math.min(totalPages, page + 1))}
            disabled={page === totalPages}
            className="pagination-btn"
            title="下一页"
          >
            <ChevronRight size={16} />
          </button>
        </div>
      )}

      {/* Confirmation Modal */}
      <Modal
        isOpen={isDeleteModalOpen}
        onClose={() => setIsDeleteModalOpen(false)}
        title="确认删除"
        footer={
          <>
            <button 
              className="modal-btn modal-btn-secondary" 
              onClick={() => setIsDeleteModalOpen(false)}
            >
              取消
            </button>
            <button 
              className="modal-btn modal-btn-danger" 
              onClick={handleBatchDelete}
              disabled={isDeleting}
            >
              {isDeleting ? '正在删除...' : '确认删除'}
            </button>
          </>
        }
      >
        <p>你确定要删除选中的 <strong>{selectedIds.size}</strong> 条反馈记录吗？此操作不可撤销。</p>
      </Modal>

      {/* Context Viewer Modal */}
      <Modal
        isOpen={!!selectedContext}
        onClose={() => setSelectedContext(null)}
        title="💬 上下文记录 (最近 5 条)"
        footer={
          <button 
            className="modal-btn modal-btn-secondary" 
            onClick={() => setSelectedContext(null)}
          >
            关闭
          </button>
        }
      >
        <div className="context-snapshot-viewer">
          {selectedContext && selectedContext.map((msg, idx) => (
            <div key={idx} className={`context-msg ${msg.role}`}>
              <div className="msg-role-tag">{msg.role === 'user' ? '👤 User' : '🤖 Assistant'}</div>
              <div className="msg-text-content">{msg.content}</div>
            </div>
          ))}
        </div>
      </Modal>
      </div>
    </div>
  );
};

export default FeedbackAnalysis;
