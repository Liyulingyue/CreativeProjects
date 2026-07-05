import { useState } from 'react';
import type { PlanCellPayload } from '../types';

interface Props {
  city: string;
  initialStartDate?: string;
  initialEndDate?: string;
  onGenerated: (result: PlanCellPayload) => void;
  onClose: () => void;
}

const PREFERENCE_TAGS = ['亲子', '美食', '户外', '人文', '自然', '放松', '购物', '摄影'];

export function PlanGenerateModal({ city, initialStartDate, initialEndDate, onGenerated, onClose }: Props) {
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [preferenceText, setPreferenceText] = useState('');
  const [startDate, setStartDate] = useState(initialStartDate || '');
  const [endDate, setEndDate] = useState(initialEndDate || '');
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function toggleTag(tag: string) {
    setSelectedTags(prev =>
      prev.includes(tag) ? prev.filter(t => t !== tag) : [...prev, tag]
    );
  }

  async function handleGenerate() {
    if (!startDate || !endDate) {
      setError('请填写出发和返回日期');
      return;
    }
    setError(null);
    setIsGenerating(true);
    try {
      const res = await fetch('/api/plan/recommend', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          city,
          start_date: startDate,
          end_date: endDate,
          preference_tags: selectedTags,
          preference_text: preferenceText || undefined,
        }),
      });
      const data: PlanCellPayload = await res.json();
      if (!data.success) {
        setError(data.error || '生成失败，请重试');
        return;
      }
      onGenerated(data);
      onClose();
    } catch (e) {
      setError('网络错误，请检查连接后重试');
    } finally {
      setIsGenerating(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={e => e.stopPropagation()} style={{ maxWidth: 480 }}>
        <div className="modal-header">
          <h2>生成我的旅行方案</h2>
          <button className="modal-close" onClick={onClose}>×</button>
        </div>

        <div className="modal-body">
          <div style={{ marginBottom: 16 }}>
            <div style={{ fontSize: 13, color: 'var(--text-secondary)', marginBottom: 6 }}>
              目标城市
            </div>
            <div style={{ fontWeight: 600, fontSize: 16 }}>{city}</div>
          </div>

          <div style={{ marginBottom: 16 }}>
            <div style={{ fontSize: 13, color: 'var(--text-secondary)', marginBottom: 6 }}>
              出行日期
            </div>
            <div style={{ display: 'flex', gap: 8 }}>
              <input
                type="date"
                className="form-input"
                style={{ flex: 1 }}
                value={startDate}
                onChange={e => setStartDate(e.target.value)}
              />
              <span style={{ color: 'var(--text-muted)', lineHeight: '36px' }}>至</span>
              <input
                type="date"
                className="form-input"
                style={{ flex: 1 }}
                value={endDate}
                onChange={e => setEndDate(e.target.value)}
              />
            </div>
          </div>

          <div style={{ marginBottom: 16 }}>
            <div style={{ fontSize: 13, color: 'var(--text-secondary)', marginBottom: 6 }}>
              偏好标签（可多选）
            </div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
              {PREFERENCE_TAGS.map(tag => (
                <button
                  key={tag}
                  onClick={() => toggleTag(tag)}
                  style={{
                    padding: '4px 12px',
                    borderRadius: 16,
                    border: selectedTags.includes(tag) ? '1.5px solid var(--primary)' : '1px solid var(--border)',
                    background: selectedTags.includes(tag) ? 'var(--primary-light)' : 'transparent',
                    color: selectedTags.includes(tag) ? 'var(--primary)' : 'var(--text-secondary)',
                    fontSize: 13,
                    cursor: 'pointer',
                  }}
                >
                  {tag}
                </button>
              ))}
            </div>
          </div>

          <div style={{ marginBottom: 20 }}>
            <div style={{ fontSize: 13, color: 'var(--text-secondary)', marginBottom: 6 }}>
              补充需求（选填）
            </div>
            <textarea
              className="form-input"
              rows={3}
              placeholder="例如：带老人和孩子，不想走太多路，喜欢自然风光和美食，避开极端天气……"
              value={preferenceText}
              onChange={e => setPreferenceText(e.target.value)}
              style={{ resize: 'vertical', width: '100%', boxSizing: 'border-box' }}
            />
          </div>

          {error && (
            <div style={{ color: 'var(--error)', fontSize: 13, marginBottom: 12 }}>
              {error}
            </div>
          )}

          <button
            className="btn-primary"
            style={{ width: '100%', height: 44, fontSize: 15 }}
            disabled={isGenerating || !startDate || !endDate}
            onClick={handleGenerate}
          >
            {isGenerating ? '生成中…' : '生成我的专属方案'}
          </button>
        </div>
      </div>
    </div>
  );
}
