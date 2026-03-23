import React from 'react';
import { Server, LayoutDashboard, Container, Activity as ActivityIcon } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { cn } from '../lib/utils';

interface NavTab {
  id: string;
  icon: LucideIcon;
  label: string;
}

interface StatsHeroProps {
  activeTab: string;
  setActiveTab: (tab: any) => void;
  navTabs: NavTab[];
  highlightChips: Array<{
    label: string;
    value: string | number;
    detail: string;
    icon: React.ReactNode;
  }>;
  signalChips: Array<{
    label: string;
    value: string | number;
    detail: string;
  }>;
}

export const StatsHero: React.FC<StatsHeroProps> = ({
  activeTab,
  setActiveTab,
  navTabs,
  highlightChips,
  signalChips
}) => {
  return (
    <>
      <header className="apple-header">
        <div className="max-w-6xl mx-auto px-6 h-16 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 bg-blue-600 rounded-lg flex items-center justify-center text-white shadow-lg shadow-blue-500/20">
              <Server size={18} strokeWidth={2.5} />
            </div>
            <span className="font-bold tracking-tight text-lg">OhMyDashboard</span>
          </div>
          
          <nav className="hidden md:flex items-center gap-1">
            {navTabs.map(tab => {
              const Icon = tab.icon;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id as any)}
                  className={cn(
                    'tab-pill',
                    activeTab === tab.id ? 'tab-pill-active' : 'tab-pill-muted'
                  )}
                >
                  <Icon size={16} />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </nav>

          <div className="flex items-center gap-4">
            <div className="h-8 w-[1px] bg-gray-200 mx-2" />
            <div className="flex flex-col items-end">
              <span className="text-[10px] uppercase font-bold text-gray-400 leading-none">Status</span>
              <span className="text-xs font-bold text-emerald-500">Live Connection</span>
            </div>
          </div>
        </div>
      </header>

      <section className="hero-panel">
        <div className="hero-glow" />
        <div className="relative z-10">
          <p className="text-[10px] uppercase font-black tracking-[0.3em] text-blue-600 mb-4 px-1">System Control Center</p>
          <h1 className="text-4xl md:text-5xl font-black tracking-tight text-gray-900 mb-4">
            Insight <span className="text-blue-600">Pulse.</span>
          </h1>
          <p className="text-lg text-gray-500 max-w-2xl font-medium leading-relaxed">
            实时洞察宿主机的 CPU、内存、磁盘和容器状态。所有数据在前端自动同步，带有灵动动画与 Apple 风格设计。
          </p>

          <div className="mt-10 grid grid-cols-1 sm:grid-cols-3 gap-6">
            {highlightChips.map(chip => (
              <div key={chip.label} className="highlight-chip">
                <div className="flex items-center gap-2 text-xs font-bold text-gray-400 uppercase tracking-wider">
                  {chip.icon}
                  <span>{chip.label}</span>
                </div>
                <div className="text-3xl font-black text-gray-900 mt-3">{chip.value}</div>
                <div className="text-xs font-medium text-gray-400 mt-1">{chip.detail}</div>
              </div>
            ))}
          </div>

          <div className="signal-row mt-10">
            {signalChips.map(signal => (
              <div key={signal.label} className="signal-chip">
                <div className="text-[10px] uppercase font-black tracking-widest text-gray-400">{signal.label}</div>
                <div className="text-xl font-bold text-gray-900 mt-1">{signal.value}</div>
                <div className="text-[11px] text-gray-500 font-medium mt-0.5">{signal.detail}</div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Mobile Nav */}
      <nav className="md:hidden flex items-center justify-center gap-2 mb-8 flex-wrap">
        {navTabs.map(tab => {
          const Icon = tab.icon;
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id as any)}
              className={cn(
                'tab-pill',
                activeTab === tab.id ? 'tab-pill-active' : 'tab-pill-muted'
              )}
            >
              <Icon size={16} />
              <span>{tab.label}</span>
            </button>
          );
        })}
      </nav>
    </>
  );
};
