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

const QUICK_PREFERENCES = [
  '自然风光', '人文历史', '美食',
  '亲子', '户外探险', '网红打卡',
];

export function FilterModal({ onApply, onClose }: Props) {
  const today = new Date();
  const [startDate, setStartDate] = useState(format(addDays(today, 1)));
  const [endDate, setEndDate] = useState(format(addDays(today, 3)));
  const [preference, setPreference] = useState('');
  const [selectedChips, setSelectedChips] = useState<string[]>([]);

  const quickRanges = [
    { label: '明天', days: 1 },
    { label: '周末', days: 2 },
    { label: '3天', days: 3 },
    { label: '5天', days: 5 },
    { label: '下周', days: 7 },
  ];

  const toggleChip = (chip: string) => {
    setSelectedChips((prev) =>
      prev.includes(chip) ? prev.filter((c) => c !== chip) : [...prev, chip]
    );
  };

  const handleApply = () => {
    const combined = [...selectedChips, preference].filter(Boolean).join('，');
    onApply({ startDate, endDate, preference: combined });
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
            <h3>偏好标签</h3>
            <div className="chip-group">
              {QUICK_PREFERENCES.map((chip) => (
                <button
                  key={chip}
                  className={`chip ${selectedChips.includes(chip) ? 'active' : ''}`}
                  onClick={() => toggleChip(chip)}
                >
                  {chip}
                </button>
              ))}
            </div>
          </div>

          <div className="filter-section">
            <h3>一句话补充偏好</h3>
            <textarea
              className="preference-input"
              placeholder="例如：住宿要求离地铁近、避开网红店..."
              value={preference}
              onChange={(e) => setPreference(e.target.value)}
              rows={2}
            />
            <p className="preference-hint">在标签基础上补充描述，与标签合并参与匹配</p>
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