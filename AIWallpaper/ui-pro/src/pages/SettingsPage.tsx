import React from 'react';

interface SettingsPageProps {
  uiMode?: 'lite' | 'pro';
  setUiMode?: (v: 'lite' | 'pro') => void;
  apiKey: string;
  setApiKey: (v: string) => void;
  enableCache: boolean;
  setEnableCache: (v: boolean) => void;
  cacheLimit: number;
  setCacheLimit: (v: number) => void;
  galleryPath: string;
  setGalleryPath: (v: string) => void;
  imageSize: string;
  setImageSize: (v: string) => void;
  handleSaveKey: () => void;
  peUrl: string;
  setPeUrl: (v: string) => void;
  peKey: string;
  setPeKey: (v: string) => void;
  peModel: string;
  setPeModel: (v: string) => void;
  autoStart: boolean;
  setAutoStart: (v: boolean) => void;
}

const SettingsPage: React.FC<SettingsPageProps> = ({
  uiMode = 'pro',
  apiKey, setApiKey,
  enableCache, setEnableCache,
  cacheLimit, setCacheLimit,
  galleryPath, setGalleryPath,
  imageSize, setImageSize,
  handleSaveKey,
  peUrl, setPeUrl,
  peKey, setPeKey,
  peModel, setPeModel,
  autoStart, setAutoStart,
}) => {
  const [showApiKey, setShowApiKey] = React.useState(false);
  const [saveMsg, setSaveMsg] = React.useState<string | null>(null);

  const sizeOptions = [
    { label: "自动识别 (推荐)", value: "auto" },
    { label: "1024x1024 (正方形)", value: "1024x1024" },
    { label: "1376x768 (宽屏)", value: "1376x768" },
    { label: "1264x848 (3:2)", value: "1264x848" },
    { label: "1200x896 (4:3)", value: "1200x896" },
    { label: "896x1200 (竖屏 3:4)", value: "896x1200" },
    { label: "848x1264 (竖屏 2:3)", value: "848x1264" },
    { label: "768x1376 (手机竖屏)", value: "768x1376" },
  ];

  return (
    <div className="max-w-xl mx-auto space-y-10 animate-in fade-in slide-in-from-bottom-6 duration-700">
      {/* API 配置(文生图) */}
      <div>
        <h3 className="text-xs font-black text-slate-400 uppercase tracking-widest mb-6">AI Studio API Token（文生图）</h3>
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
              <p className="text-[10px] text-slate-400 px-1 font-medium italic">您的 Token 将安全地存储在本地配置文件中，仅用于壁纸生成请求。若服务区分不同功能，请在 Token 后添加文生图后缀以区分。</p>
        </div>
      </div>

      {/* Prompt Enhance (文生文) 配置 */}
      <div>
        <h3 className="text-xs font-black text-slate-400 uppercase tracking-widest mb-6">Prompt Enhance（文生文）</h3>
        <div className="bg-white border border-slate-200 rounded-[2rem] p-8 shadow-lg space-y-4">
          <div className="flex items-start gap-3 pb-2">
            <div className="p-2 bg-slate-100 rounded-xl text-slate-600">
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" /></svg>
            </div>
            <div className="flex-1">
              <p className="text-sm font-semibold text-slate-700 font-[system-ui]">OpenAI 格式的 PE 服务配置</p>
              <p className="text-[10px] text-slate-400 mt-0.5">只需填写到 https://.../v1 即可，框架会自动补全 /chat/completions；同时提供 Key 与 Model。</p>
            </div>
          </div>

          <div className="space-y-3">
            <input
              type="text"
              value={peUrl}
              onChange={(e) => setPeUrl(e.target.value)}
              placeholder="接口地址（例如 https://api.openai.com/v1 ）"
              className="w-full bg-slate-50 border border-slate-200 rounded-2xl p-4 text-sm focus:ring-4 focus:ring-blue-500/10 focus:border-blue-500/50 outline-none transition-all"
            />
            <input
              type="text"
              value={peKey}
              onChange={(e) => setPeKey(e.target.value)}
              placeholder="API Key (Bearer)"
              className="w-full bg-slate-50 border border-slate-200 rounded-2xl p-4 text-sm focus:ring-4 focus:ring-blue-500/10 focus:border-blue-500/50 outline-none transition-all"
            />
            <input
              type="text"
              value={peModel}
              onChange={(e) => setPeModel(e.target.value)}
              placeholder="Model 名称（例如 gpt-4o-mini 或 gpt-3.5-turbo）"
              className="w-full bg-slate-50 border border-slate-200 rounded-2xl p-4 text-sm focus:ring-4 focus:ring-blue-500/10 focus:border-blue-500/50 outline-none transition-all"
            />
          </div>
        </div>
      </div>

      {/* 生成规格配置 */}
      <div className={uiMode === 'lite' ? 'hidden' : ''}>
        <h3 className="text-xs font-black text-slate-400 uppercase tracking-widest mb-6">图片生成规格</h3>
        <div className="bg-white border border-slate-200 rounded-[2rem] p-8 shadow-lg space-y-6">
          <div className="flex items-start gap-3">
            <div className="p-2 bg-slate-100 rounded-xl text-slate-600">
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M9 21V9"/></svg>
            </div>
            <div className="flex-1">
              <p className="text-sm font-semibold text-slate-700 font-[system-ui]">画面比例与分辨率</p>
              <p className="text-[10px] text-slate-400 mt-0.5">选择生图尺寸并点击下方“保存配置”生效。自动识别将根据您的屏幕分辨率匹配规格。</p>
            </div>
          </div>
          <div className="relative">
            <select
              value={imageSize}
              onChange={(e) => setImageSize(e.target.value)}
              className="w-full bg-slate-50 border border-slate-200 rounded-2xl p-4 pr-10 text-sm focus:ring-4 focus:ring-blue-500/10 focus:border-blue-500/50 outline-none transition-all appearance-none cursor-pointer font-medium text-slate-700"
            >
              {sizeOptions.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label} ({opt.value === "auto" ? "自动识别" : opt.value})
                </option>
              ))}
            </select>
            <div className="absolute right-4 top-1/2 -translate-y-1/2 pointer-events-none text-slate-400">
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg>
            </div>
          </div>
        </div>
      </div>

      {/* 本地存储配置 */}
      <div className={uiMode === 'lite' ? 'hidden' : ''}>
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

      {/* 开机自启 */}
      <div className={uiMode === 'lite' ? 'hidden' : ''}>
        <h3 className="text-xs font-black text-slate-400 uppercase tracking-widest mb-6">系统行为</h3>
        <div className="bg-white border border-slate-200 rounded-[2rem] p-8 shadow-lg">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-slate-100 rounded-xl text-slate-600">
                <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 2v10"/><path d="M18.4 6.6a9 9 0 1 1-12.77.04"/></svg>
              </div>
              <div>
                <p className="text-sm font-semibold text-slate-700">开机自启动</p>
                <p className="text-[10px] text-slate-400">登录 Windows 后自动启动 AIWallpaper</p>
              </div>
            </div>
            <button
              onClick={() => {
                const next = !autoStart;
                setAutoStart(next);
                setTimeout(() => handleSaveKey(), 0);
              }}
              className={`w-12 h-6 rounded-full transition-colors relative ${autoStart ? 'bg-blue-600' : 'bg-slate-200'}`}
            >
              <div className={`absolute top-1 w-4 h-4 bg-white rounded-full transition-all ${autoStart ? 'left-7' : 'left-1'}`}></div>
            </button>
          </div>
        </div>
      </div>

      <div className="pt-6">
        <button
          onClick={async () => {
              try {
                await Promise.resolve(handleSaveKey());
                setSaveMsg("配置已保存");
              } catch (e) {
                setSaveMsg("保存失败，请重试");
              }
            }}
          className="w-full bg-slate-900 hover:bg-black text-white py-5 rounded-[2rem] font-black text-sm transition-all shadow-2xl shadow-blue-900/10 active:scale-[0.98] flex items-center justify-center gap-2 group"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" className="group-hover:scale-110 transition-transform"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/><polyline points="17 21 17 13 7 13 7 21"/><polyline points="7 3 7 8 15 8"/></svg>
          保存所有配置
        </button>
        <p className="text-center text-[10px] text-slate-400 mt-4 font-medium italic opacity-60">
          保存后将立即更新本地 config.json 并同步至后台服务
        </p>

        {saveMsg && (
          <div className="fixed inset-0 z-50 flex items-center justify-center">
            <div className="absolute inset-0 bg-black/40 backdrop-blur-sm" onClick={() => setSaveMsg(null)} />
            <div className="relative bg-white rounded-2xl p-6 w-[320px] mx-4 shadow-2xl border">
              <div className="flex items-center gap-3">
                <div className={`w-10 h-10 flex items-center justify-center rounded-full ${saveMsg === '配置已保存' ? 'bg-green-100' : 'bg-red-100'}`}>
                  {saveMsg === '配置已保存' ? (
                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-green-600"><path d="M20 6L9 17l-5-5"/></svg>
                  ) : (
                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-red-600"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
                  )}
                </div>
                <div className="flex-1">
                  <div className="text-sm font-semibold text-slate-800">{saveMsg}</div>
                  <div className="text-[11px] text-slate-400 mt-1">已同步到本地配置。</div>
                </div>
              </div>
              <div className="mt-4 flex justify-end">
                <button onClick={() => setSaveMsg(null)} className="px-4 py-2 bg-slate-100 rounded-lg text-sm hover:bg-slate-200">确定</button>
              </div>
            </div>
          </div>
        )}
      </div>

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
