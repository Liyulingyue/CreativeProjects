import { useState } from 'react';
import { Icon } from './TopBar';

export default function InfoPanel({ content }) {
  const [collapsed, setCollapsed] = useState(false);

  return (
    <section className={`info-panel ${collapsed ? 'collapsed' : ''}`}>
      <div className="info-header" onClick={() => setCollapsed((c) => !c)}>
        <span className="info-title">分析结果</span>
        <span className={`chevron ${collapsed ? 'rotated' : ''}`}>
          <Icon name="chevron" size={18} />
        </span>
      </div>
      <div className="info-body">
        <pre>{content || '等待分析…'}</pre>
      </div>
    </section>
  );
}
