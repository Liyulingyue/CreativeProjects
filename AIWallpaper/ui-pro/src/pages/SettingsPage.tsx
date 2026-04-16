import React from 'react';

interface SettingsPageProps {
  apiKey: string;
  setApiKey: (v: string) => void;
  enableCache: boolean;
  setEnableCache: (v: boolean) => void;
  cacheLimit: number;
  setCacheLimit: (v: number) => void;
  galleryPath: string;
  setGalleryPath: (v: string) => void;
  handleSaveKey: () => void;
}

const SettingsPage: React.FC<SettingsPageProps> = ({
  apiKey, setApiKey,
  enableCache, setEnableCache,
  cacheLimit, setCacheLimit,
  galleryPath, setGalleryPath,
  handleSaveKey
}) => {
  const [showApiKey, setShowApiKey] = React.useState(false);

  return (
    <div className="max-w-xl mx-auto space-y-10 animate-in fade-in slide-in-from-bottom-6 duration-700">
      {/* API 配置 */}
      <div>
        <h3 className="text-xs font-black text-slate-400 uppercase tracking-widest mb-6">AI Studio API Token</h3>
        <div className="bg-white border border-slate-200 rounded-[2rem] p-8 shadow-lg space-y-4">
          <div className="flex items-start gap-3 pb-2">
            <div className="p-2 bg-slate-100 rounded-xl text-slate-600">
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" /></svg>
            </div>
            <div className="flex-1">
              <p className="text-sm font-semibold text-slate-700 font-[system-ui]">API Studio 身份凭证</p>
              <a href="https://aistudio.baidu.com/account/accessToken" target="_blank" rel="noreferrer" className="text-[10px] text-blue-500 hover:text-blue-700 flex items-center gap-1 mt-0.5 transition">
                <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" /><polyline points="15 3 21 3 21 9" /><line x1="10" x2="21" y1="14" y2="3" /></svg>
                前往 aistudio.baidu.com 获取 Token
              </a>
            </div>
          </div>
          <div className="relative group">
            <input
              type={showApiKey ? "text" : "password"}
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="粘贴您的 API Key..."
              className="w-full bg-slate-50 border border-slate-200 rounded-2xl p-4 pr-12 text-sm focus:ring-4 focus:ring-blue-500/10 focus:border-blue-500/50 outline-none transition-all"
            />
            <button
              onClick={() => setShowApiKey(!showApiKey)}
              className="absolute right-4 top-1/2 -translate-y-1/2 p-2 text-slate-400 hover:text-slate-600 transition-colors"
              title={showApiKey ? "隐藏 Token" : "显示 Token"}
            >
              {showApiKey ? (
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M9.88 9.88 3.59 3.59" /><path d="M21 3.1a3.57 3.57 0 0 1-4.8 4.8L12 12l-4.2-4.2a3.57 3.57 0 0 1-4.8 4.8L2.1 13.5a1 1 0 0 1 0-1.4L15.6 1.4a1 1 0 0 1 1.4 0l1.4 1.4Z" /><path d="m11.5 11.5-6.4 6.4a2.12 2.12 0 0 0-3 3 2.12 2.12 0 0 0 3 3l6.4-6.4" /><path d="M14.5 14.5l6.4 6.4a2.12 2.12 0 0 0 3-3 2.12 2.12 0 0 0-3-3l-6.4 6.4" /><circle cx="12" cy="12" r="3" /></svg>
              ) : (
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" /><circle cx="12" cy="12" r="3" /></svg>
              )}
            </button>
          </div>
          <p className="text-[10px] text-slate-400 px-1 font-medium italic">您的 Token 将安全地存储在本地配置文件中，仅用于壁纸生成请求。</p>
        </div>
      </div>

      {/* 本地存储配置 */}
      <div>
        <h3 className="text-xs font-black text-slate-400 uppercase tracking-widest mb-6">本地存储配置</h3>
        <div className="bg-white border border-slate-200 rounded-[2rem] p-8 shadow-lg space-y-8">
          {/* 画廊路径 */}
          <div className="space-y-4">
            <div className="flex items-start gap-3">
              <div className="p-2 bg-slate-100 rounded-xl text-slate-600">
                <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" /></svg>
              </div>
              <div className="flex-1">
                <p className="text-sm font-semibold text-slate-700 font-[system-ui]">自定义画廊路径</p>
                <p className="text-[10px] text-slate-400 mt-0.5">留空则使用系统默认的“图片/AIWallpaper”文件夹</p>
              </div>
            </div>
            <div className="relative">
              <input
                type="text"
                value={galleryPath}
                onChange={(e) => setGalleryPath(e.target.value)}
                placeholder="例如: D:\Wallpapers..."
                className="w-full bg-slate-50 border border-slate-200 rounded-2xl p-4 text-sm focus:ring-4 focus:ring-blue-500/10 focus:border-blue-500/50 outline-none transition-all placeholder:italic"
              />
              {galleryPath && (
                <button 
                  onClick={() => setGalleryPath("")}
                  className="absolute right-4 top-1/2 -translate-y-1/2 p-2 text-slate-300 hover:text-slate-500 transition-colors"
                  title="重置为默认"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10" /><path d="m15 9-6 6" /><path d="m9 9 6 6" /></svg>
                </button>
              )}
            </div>
          </div>

          <div className="h-px bg-slate-100 mx-2"></div>

          {/* 缓存设置 */}
          <div className="space-y-6">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className="p-2 bg-slate-100 rounded-xl text-slate-600">
                  <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v3" /><path d="m21 12-1 8H4l-1-8h18Z" /><path d="M10 12V8" /><path d="M14 12V8" /></svg>
                </div>
                <div>
                  <p className="text-sm font-semibold text-slate-700">启用本地缓存</p>
                  <p className="text-[10px] text-slate-400">保存生成历史以供画廊预览</p>
                </div>
              </div>
              <button
                onClick={() => setEnableCache(!enableCache)}
                className={`w-12 h-6 rounded-full transition-colors relative ${enableCache ? 'bg-blue-600' : 'bg-slate-200'}`}
              >
                <div className={`absolute top-1 w-4 h-4 bg-white rounded-full transition-all ${enableCache ? 'left-7' : 'left-1'}`}></div>
              </button>
            </div>
            {enableCache && (
              <div className="space-y-4 animate-in fade-in slide-in-from-top-2">
                <label className="text-[11px] font-bold text-slate-400 px-1 uppercase flex justify-between">
                  <span>最大缓存数量</span>
                  <span className="text-blue-600">{cacheLimit} 张</span>
                </label>
                <input
                  type="range" min="10" max="500" step="10"
                  value={cacheLimit}
                  onChange={(e) => setCacheLimit(parseInt(e.target.value))}
                  className="w-full h-1.5 bg-slate-100 rounded-lg appearance-none cursor-pointer accent-blue-600"
                />
              </div>
            )}
          </div>
        </div>
      </div>

      <button onClick={handleSaveKey} className="w-full bg-slate-900 hover:bg-black text-white py-4 rounded-2xl font-bold text-sm transition-all shadow-xl shadow-slate-100 active:scale-[0.98]">应用并保存所有设置</button>

      {/* 系统信息 */}

      <div>
        <h3 className="text-xs font-black text-slate-400 uppercase tracking-widest mb-6">系统信息</h3>
        <div className="bg-white border border-slate-200 rounded-[2rem] p-8 shadow-lg space-y-5">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-slate-100 rounded-xl text-slate-600">
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect width="20" height="14" x="2" y="3" rx="2" /><line x1="8" x2="16" y1="21" y2="21" /><line x1="12" x2="12" y1="17" y2="21" /></svg>
            </div>
            <div>
              <p className="text-sm font-semibold text-slate-700">AIWallpaper Pro v1.1.0</p>
              <p className="text-[10px] text-slate-400 uppercase">STABLE</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default SettingsPage;
