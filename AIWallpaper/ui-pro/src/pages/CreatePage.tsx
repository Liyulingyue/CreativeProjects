import React, { useState } from 'react';
import ImageEditorModal from '../components/ImageEditorModal';

interface CreatePageProps {
  uiMode?: 'lite' | 'pro';
  prompt: string;
  setPrompt: (v: string) => void;
  handleRandomPrompt: () => void;
  handleGenerate: () => void;
  isGenerating: boolean;
  isEnhancing: boolean;
  setIsEnhancing: (v: boolean) => void;
  statusMsg: string;
  previewUrl: string;
  sendIpc: (cmd: string, arg?: any) => void;
  setActiveTab: (tab: string) => void;
  setShowViewer: (v: boolean) => void;
  autoRefreshHours: number;
  peUrl: string;
  peKey: string;
  peModel: string;
  showMessage: (msg: string, duration?: number) => void;
}

const CreatePage: React.FC<CreatePageProps> = ({
  uiMode = 'pro', prompt, setPrompt, handleRandomPrompt, handleGenerate,
  isGenerating, isEnhancing, setIsEnhancing, statusMsg, previewUrl, sendIpc, setActiveTab,
  setShowViewer, autoRefreshHours, peUrl, peKey, showMessage
}) => {
  const [isImporting, setIsImporting] = React.useState(false);
  const [showEditor, setShowEditor] = useState(false);

  const handleEditSave = (base64Data: string, asWallpaper: boolean = false) => {
    setShowEditor(false);
    // 通过 IPC 发送数据给 Rust 进行保存，默认仅保存到画廊
    sendIpc("save_edited_image", { data: base64Data, set_as_wallpaper: asWallpaper });
  };

  if (uiMode === 'lite') {
    return (
      <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
        <div className="bg-white border border-slate-200 rounded-3xl p-6 shadow-lg">
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder="输入描述..."
            className="w-full bg-slate-50 border border-slate-100 rounded-2xl p-4 text-lg min-h-[120px] focus:ring-2 focus:ring-blue-500/20 focus:border-blue-500/30 outline-none transition-all resize-none placeholder:text-slate-300"
          />
          
          <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
            <div className="flex gap-2">
              <button 
                onClick={handleRandomPrompt} 
                className="p-2 rounded-xl bg-slate-50 border border-slate-200 hover:bg-slate-100 text-amber-500 transition-colors" 
                title="灵感骰子"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><rect width="18" height="18" x="3" y="3" rx="2" ry="2" /><path d="M7 7h.01" /><path d="M17 7h.01" /><path d="M7 17h.01" /><path d="M17 17h.01" /><path d="M12 12h.01" /></svg>
              </button>
              <button 
                onClick={() => {
                  if (!prompt.trim()) {
                    showMessage('请先输入描述', 3000);
                    return;
                  }
                  if (!peKey || !peUrl) {
                    showMessage('请先在配置中设置 PE 参数', 3000);
                    return;
                  }
                  setIsEnhancing(true);
                  sendIpc("prompt_enhance", prompt);
                }}
                disabled={isEnhancing}
                className="p-2 rounded-xl bg-slate-50 border border-slate-200 hover:bg-slate-100 text-emerald-500 disabled:opacity-50 transition-colors" 
                title="优化表述"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="m21.64 3.64-1.28-1.28a1.21 1.21 0 0 0-1.72 0L2.36 18.64a1.21 1.21 0 0 0 0 1.72l1.28 1.28a1.2 1.2 0 0 0 1.72 0L21.64 5.36a1.2 1.2 0 0 0 0-1.72Z"/><path d="m14 7 3 3"/><path d="M5 6v4"/><path d="M19 14v4"/><path d="M10 2v2"/><path d="M7 8H3"/><path d="M21 16h-4"/><path d="M11 3H9"/></svg>
              </button>
            </div>

            <button 
              onClick={handleGenerate} 
              disabled={isGenerating || !prompt.trim()} 
              className="px-8 py-2 bg-blue-600 text-white rounded-xl font-bold hover:bg-blue-700 disabled:bg-slate-300 transition-all active:scale-95 shadow-md shadow-blue-500/20"
            >
              {isGenerating ? "生成中..." : "开始生成"}
            </button>
          </div>
        </div>

        {previewUrl && (
          <div className="relative group rounded-3xl overflow-hidden border border-slate-200 shadow-xl bg-white aspect-video">
            <img src={previewUrl} className="w-full h-full object-cover" alt="Preview" />
            <div className="absolute inset-x-0 bottom-0 p-4 bg-gradient-to-t from-black/60 to-transparent opacity-0 group-hover:opacity-100 transition-opacity flex justify-between items-center">
              <span className="text-white text-xs font-medium">预览图</span>
              <button 
                onClick={() => setShowViewer(true)}
                className="p-2 bg-white/20 backdrop-blur-md rounded-lg text-white hover:bg-white/40 transition-colors"
              >
                查看大图
              </button>
            </div>
          </div>
        )}
        
        {statusMsg && <p className="text-center text-sm font-medium text-blue-600 animate-pulse">{statusMsg}</p>}
      </div>
    );
  }

  return (
    <div className="space-y-10 animate-in fade-in slide-in-from-bottom-6 duration-700">
      {/* 图片编辑器 Modal */}
      {showEditor && previewUrl && (
        <ImageEditorModal 
          imageUrl={previewUrl} 
          onSave={handleEditSave} 
          onCancel={() => setShowEditor(false)} 
        />
      )}
      <div className="bg-white border border-slate-200 rounded-[2.5rem] p-10 shadow-xl relative overflow-hidden group">
        <div className="absolute top-0 right-0 p-8 opacity-5 pointer-events-none group-hover:scale-110 transition-transform duration-1000">
          <svg xmlns="http://www.w3.org/2000/svg" width="120" height="120" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" strokeLinecap="round" strokeLinejoin="round" className="text-blue-500"><path d="M12 3v19" /><path d="M5 8h14" /><path d="M15 21a3 3 0 0 0-3-3 3 3 0 0 0-3 3" /><path d="M19 12a3 3 0 0 0-3-3 3 3 0 0 0-3 3" /></svg>
        </div>
        <h2 className="text-3xl font-bold mb-8 flex items-center gap-3 text-slate-900"><span className="w-1.5 h-8 bg-blue-600 rounded-full"></span>开启创作灵感</h2>
        <textarea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          placeholder="描述你想要的画面..."
          className="w-full bg-slate-50 border border-slate-200 rounded-3xl p-6 text-xl min-h-[180px] focus:ring-4 focus:ring-blue-500/10 focus:border-blue-500/50 outline-none transition-all resize-none shadow-inner placeholder:text-slate-300 leading-relaxed text-slate-700"
        />
        <div className="mt-10 flex items-center justify-between">
          <div className="flex items-center gap-4">
            <button onClick={handleRandomPrompt} className="flex items-center gap-2 px-5 py-2.5 rounded-2xl bg-white border border-slate-200 hover:bg-slate-50 text-sm font-semibold transition-all active:scale-95 text-slate-600">
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="text-amber-500"><rect width="18" height="18" x="3" y="3" rx="2" ry="2" /><path d="M7 7h.01" /><path d="M17 7h.01" /><path d="M7 17h.01" /><path d="M17 17h.01" /><path d="M12 12h.01" /></svg>
              灵感骰子
            </button>
            <button
              onClick={() => {
                if (!prompt.trim()) {
                  showMessage('提示：请输入要优化的提示词后再点击“优化表述”。', 3000);
                  return;
                }
                if (!peKey || !peUrl) {
                  showMessage('提示：请先在设置中填写 Prompt Enhance 的 API 地址与 Key（Settings）。', 4000);
                  return;
                }
                setIsEnhancing(true);
                sendIpc("prompt_enhance", prompt);
              }}
              disabled={isEnhancing || isGenerating}
              className="flex items-center gap-2 px-5 py-2.5 rounded-2xl bg-white border border-slate-200 hover:bg-slate-50 text-sm font-semibold transition-all active:scale-95 text-slate-600 disabled:opacity-50"
              title="优化当前提示词的表述"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="text-emerald-500"><path d="m21.64 3.64-1.28-1.28a1.21 1.21 0 0 0-1.72 0L2.36 18.64a1.21 1.21 0 0 0 0 1.72l1.28 1.28a1.2 1.2 0 0 0 1.72 0L21.64 5.36a1.2 1.2 0 0 0 0-1.72Z"/><path d="m14 7 3 3"/><path d="M5 6v4"/><path d="M19 14v4"/><path d="M10 2v2"/><path d="M7 8H3"/><path d="M21 16h-4"/><path d="M11 3H9"/></svg>
              {isEnhancing ? '优化中...' : '优化表述'}
            </button>
            <button 
              onClick={() => {
                setIsImporting(true);
                sendIpc("import_image");
                setTimeout(() => setIsImporting(false), 2000);
              }} 
              disabled={isImporting}
              className="flex items-center gap-2 px-5 py-2.5 rounded-2xl bg-white border border-slate-200 hover:bg-slate-50 text-sm font-semibold transition-all active:scale-95 text-slate-600 disabled:opacity-50"
              title="导入本地图片作为壁纸"
            >
              {isImporting ? (
                 <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="animate-spin text-blue-500"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>
              ) : (
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="text-blue-500">
                  <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" x2="12" y1="3" y2="15"/>
                </svg>
              )}
              {isImporting ? "正在开启..." : "导入图片"}
            </button>
          </div>
          <div className="flex items-center gap-6">
            {(statusMsg || isImporting) && (
              <span className="text-sm font-medium text-blue-600 animate-pulse">
                {isImporting ? "请在弹出的对话框中选择图片..." : statusMsg}
              </span>
            )}
            <button
              onClick={handleGenerate}
              disabled={isGenerating || !prompt.trim()}
              className={`bg-slate-900 hover:bg-black text-white px-12 py-4 rounded-[1.25rem] font-black text-xl shadow-2xl shadow-slate-200 active:scale-95 transition-all flex items-center gap-3 ${isGenerating ? "opacity-50 cursor-not-allowed" : ""}`}
            >
              {isGenerating ? "正在生成..." : "开始生成"}
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><path d="M5 12h14" /><path d="m12 5 7 7-7 7" /></svg>
            </button>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-10">
        <div className="bg-white border border-slate-200 rounded-[2.5rem] p-8 shadow-lg">
          <div className="flex items-center justify-between mb-6">
            <h3 className="text-slate-400 text-xs font-black uppercase tracking-[0.2em]">当前壁纸预览</h3>
            <div className={`w-2.5 h-2.5 rounded-full ${isGenerating ? "bg-blue-500 animate-pulse" : "bg-green-500"}`}></div>
          </div>
          <div className="aspect-video bg-slate-100 rounded-3xl overflow-hidden border border-slate-200 group relative shadow-inner flex items-center justify-center">
            {previewUrl ? <img src={previewUrl} className="w-full h-full object-cover cursor-zoom-in" alt="Preview" onClick={() => setShowViewer(true)} /> : <div className="text-slate-300 text-sm italic">等待生成...</div>}
            {previewUrl && (
              <div className="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center gap-4 border-2 border-transparent group-hover:border-blue-500/50 rounded-3xl">
                <button 
                  onClick={() => setShowViewer(true)} 
                  className="p-3 bg-white rounded-2xl text-slate-900 shadow-xl hover:scale-110 transition-transform flex items-center gap-2 font-bold px-4"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" /><line x1="11" y1="8" x2="11" y2="14" /><line x1="8" y1="11" x2="14" y2="11" /></svg>
                  放大查看
                </button>
                <button 
                  onClick={() => setShowEditor(true)} 
                  className="p-3 bg-white rounded-2xl text-blue-600 shadow-xl hover:scale-110 transition-transform flex items-center gap-2 font-bold px-4"
                  title="编辑当前预览图片"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
                  编辑绘制
                </button>
                <button 
                  onClick={() => sendIpc("save_image")} 
                  className="p-3 bg-white/20 backdrop-blur-md border border-white/30 rounded-2xl text-white shadow-xl hover:scale-110 hover:bg-white hover:text-slate-900 transition-all"
                  title="下载壁纸"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" x2="12" y1="15" y2="3" /></svg>
                </button>
              </div>
            )}
          </div>
        </div>
        <div className="bg-white border border-slate-200 rounded-[2.5rem] p-8 shadow-lg">
          <div className="flex items-center justify-between mb-6">
            <h3 className="text-slate-400 text-xs font-black uppercase tracking-[0.2em]">自动化状态</h3>
            <button onClick={() => setActiveTab("tasks")} className="text-blue-600 text-[10px] font-black hover:underline uppercase tracking-tighter">前往配置</button>
          </div>
          <div className="space-y-4">
            <div className={`flex items-center justify-between p-5 rounded-2xl border ${autoRefreshHours > 0 ? 'bg-blue-50 border-blue-100 border-l-4 border-l-blue-600' : 'bg-slate-50 border-slate-100 border-l-4 border-l-slate-300'}`}>
              <div className="flex items-center gap-4">
                <div className={`p-2 rounded-lg shadow-sm ${autoRefreshHours > 0 ? 'bg-white text-blue-600' : 'bg-white text-slate-300'}`}>
                  <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></svg>
                </div>
                <div>
                  <div className={`text-sm font-bold ${autoRefreshHours > 0 ? 'text-slate-800' : 'text-slate-400'}`}>
                    {autoRefreshHours > 0 ? '智能自动更新中' : '自动更新已关闭'}
                  </div>
                  <div className="text-[10px] text-slate-400 font-medium italic">
                    {autoRefreshHours > 0 ? `每 ${autoRefreshHours} 小时刷新一次` : '前往自动化界面开启'}
                  </div>
                </div>
              </div>
              {autoRefreshHours > 0 && (
                <div className="h-1.5 w-12 bg-blue-200 rounded-full overflow-hidden">
                  <div className="h-full bg-blue-600 w-full animate-progress-fast"></div>
                </div>
              )}
            </div>
            
            <div className="p-4 bg-slate-50 rounded-2xl border border-dashed border-slate-200 flex items-center justify-center">
               <div className="text-[10px] text-slate-400 font-bold uppercase tracking-widest">
                  后台守护进程运行中
               </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default CreatePage;
