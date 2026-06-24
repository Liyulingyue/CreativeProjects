import type { TabType } from "../types";

interface Props {
  active: TabType;
  onChange: (tab: TabType) => void;
  counts: {
    images: number;
    results: number;
  };
}

const TABS: { id: TabType; icon: string; label: string }[] = [
  { id: "images", icon: "✨", label: "分析" },
  { id: "results", icon: "📊", label: "结果" },
  { id: "settings", icon: "⚙️", label: "设置" },
];

export function BottomNav({ active, onChange, counts }: Props) {
  return (
    <nav className="bottom-nav">
      {TABS.map((tab) => {
        const isActive = active === tab.id;
        const count = tab.id === "images" ? counts.images : tab.id === "results" ? counts.results : 0;
        return (
          <button
            key={tab.id}
            className={`bottom-nav-item ${isActive ? "active" : ""}`}
            onClick={() => onChange(tab.id)}
            aria-label={tab.label}
          >
            <span className="bottom-nav-icon">{tab.icon}</span>
            <span className="bottom-nav-label">{tab.label}</span>
            {count > 0 && (
              <span className={`bottom-nav-badge ${tab.id === "results" ? "badge-accent" : ""}`}>
                {count > 99 ? "99+" : count}
              </span>
            )}
          </button>
        );
      })}
    </nav>
  );
}