import { useState } from 'react';
import { format, addDays } from '../utils/date';

interface SearchInput {
  startDate?: string;
  endDate?: string;
  duration?: number;
  style?: string;
  sortBy?: string;
  preference: string;
}

interface Props {
  onApply: (input: SearchInput) => void;
  onClose: () => void;
}

const QUICK_PREFERENCES = [
  '自然风光', '人文历史', '美食',
  '亲子', '户外探险', '网红打卡',
];

const DURATIONS = [1, 2, 3, 4, 5];
const STYLES = [
  { key: 'standard', label: '标准' },
  { key: 'family', label: '亲子' },
  { key: 'budget', label: '穷游' },
];
const SORT_OPTIONS = [
  { key: 'score', label: '综合评分' },
  { key: 'preference', label: '偏好匹配' },
  { key: 'weather', label: '天气最优' },
];

export function FilterModal({ onApply, onClose }: Props) {
  const today = new Date();
  const [withDate, setWithDate] = useState(true);
  const [startDate, setStartDate] = useState(format(addDays(today, 1)));
  const [endDate, setEndDate] = useState(format(addDays(today, 3)));
  const [duration, setDuration] = useState(3);
  const [style, setStyle] = useState('standard');
  const [sortBy, setSortBy] = useState('score');
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
    if (withDate) {
      onApply({ startDate, endDate, sortBy, preference: combined });
    } else {
      onApply({ duration, style, sortBy, preference: combined });
    }
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
            <h3>搜索模式</h3>
            <div className="seg-group">
              <button type="button" className={`seg-btn ${withDate ? 'active' : ''}`} onClick={() => setWithDate(true)}>按日期</button>
              <button type="button" className={`seg-btn ${!withDate ? 'active' : ''}`} onClick={() => setWithDate(false)}>按天数</button>
            </div>
          </div>

          {withDate ? (
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
              <div className="date-range-input" style={{ marginTop: 10 }}>
                <input type="date" value={startDate} onChange={(e) => setStartDate(e.target.value)} min={format(today)} />
                <span>至</span>
                <input type="date" value={endDate} onChange={(e) => setEndDate(e.target.value)} min={startDate} />
              </div>
            </div>
          ) : (
            <div className="filter-section">
              <h3>天数</h3>
              <div className="seg-group">
                {DURATIONS.map((d) => (
                  <button key={d} type="button" className={`seg-btn ${duration === d ? 'active' : ''}`} onClick={() => setDuration(d)}>{d}天</button>
                ))}
              </div>
              <h3 style={{ marginTop: 16 }}>风格</h3>
              <div className="seg-group">
                {STYLES.map((s) => (
                  <button key={s.key} type="button" className={`seg-btn ${style === s.key ? 'active' : ''}`} onClick={() => setStyle(s.key)}>{s.label}</button>
                ))}
              </div>
            </div>
          )}

          <div className="filter-section">
            <h3>偏好标签</h3>
            <div className="chip-group">
              {QUICK_PREFERENCES.map((chip) => (
                <button key={chip} className={`chip ${selectedChips.includes(chip) ? 'active' : ''}`} onClick={() => toggleChip(chip)}>
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
            <div className="seg-group">
              {SORT_OPTIONS.map((opt) => (
                <button key={opt.key} type="button" className={`seg-btn ${sortBy === opt.key ? 'active' : ''}`} onClick={() => setSortBy(opt.key)}>
                  {opt.label}
                </button>
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
