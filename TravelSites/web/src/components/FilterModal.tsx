import { useState } from 'react';
import { format, addDays } from '../utils/date';

interface FilterData {
  startDate: string;
  endDate: string;
  preference: string;
}

interface Props {
  onApply: (filters: FilterData) => void;
  onClose: () => void;
}

export function FilterModal({ onApply, onClose }: Props) {
  const today = new Date();
  const [startDate, setStartDate] = useState(format(addDays(today, 1)));
  const [endDate, setEndDate] = useState(format(addDays(today, 3)));
  const [preference, setPreference] = useState('');

  const quickRanges = [
    { label: '明天', days: 1 },
    { label: '周末', days: 2 },
    { label: '3天', days: 3 },
    { label: '5天', days: 5 },
    { label: '下周', days: 7 },
  ];

  const handleApply = () => {
    onApply({ startDate, endDate, preference });
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content filter-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>筛选</h2>
          <button className="modal-close" onClick={onClose}>×</button>
        </div>

        <div className="modal-body">
          <div className="filter-section">
            <h3>快捷日期</h3>
            <div className="quick-ranges">
              {quickRanges.map((r) => (
                <button
                  key={r.label}
                  className="quick-btn"
                  onClick={() => {
                    setStartDate(format(addDays(today, 1)));
                    setEndDate(format(addDays(today, r.days)));
                  }}
                >
                  {r.label}
                </button>
              ))}
            </div>
          </div>

          <div className="filter-section">
            <h3>自定义日期</h3>
            <div className="date-range-input">
              <input
                type="date"
                value={startDate}
                onChange={(e) => setStartDate(e.target.value)}
                min={format(today)}
              />
              <span>至</span>
              <input
                type="date"
                value={endDate}
                onChange={(e) => setEndDate(e.target.value)}
                min={startDate}
              />
            </div>
          </div>

          <div className="filter-section">
            <h3>一句话描述你的偏好</h3>
            <textarea
              className="preference-input"
              placeholder="例如：带孩子看自然风光、美食之旅..."
              value={preference}
              onChange={(e) => setPreference(e.target.value)}
              rows={2}
            />
            <p className="preference-hint">输入旅行偏好，系统会优先推荐匹配的行程</p>
          </div>

          <div className="filter-section">
            <h3>排序</h3>
            <div className="chip-group">
              {['综合评分', '偏好匹配', '天气最优'].map((opt) => (
                <button key={opt} className={`chip ${opt === '综合评分' ? 'active' : ''}`}>{opt}</button>
              ))}
            </div>
          </div>
        </div>

        <div className="modal-footer">
          <button className="btn btn-secondary" onClick={onClose}>取消</button>
          <button className="btn btn-primary" onClick={handleApply}>应用筛选</button>
        </div>
      </div>
    </div>
  );
}
