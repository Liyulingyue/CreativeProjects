import React, { useState } from 'react';

interface GalleryPageProps {
  galleryImages: any[];
  sendIpc: (cmd: string, arg?: any) => void;
  galleryPath: string;
  onZoom: (url: string) => void;
}

const GalleryPage: React.FC<GalleryPageProps> = ({ galleryImages, sendIpc, galleryPath, onZoom }) => {
  const displayPath = galleryPath || "Pictures/AIWallpaper";
  const [confirmDeleteName, setConfirmDeleteName] = useState<string | null>(null);
  const [applyingName, setApplyingName] = useState<string | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);

  const handleRefresh = () => {
    setIsRefreshing(true);
    sendIpc("get_gallery");
    // 模拟动画持续时间，之后自动停止，或者让父组件在加载完后重置状态
    setTimeout(() => setIsRefreshing(false), 1000);
  };
  
  return (
    <>
    <div className="space-y-10 animate-in fade-in slide-in-from-bottom-6 duration-700">
      <div className="bg-white border border-slate-200 rounded-[2.5rem] p-8 shadow-xl relative overflow-hidden group mb-10">
        <div className="absolute top-0 right-0 p-8 opacity-5 pointer-events-none group-hover:scale-110 transition-transform duration-1000">
          <svg xmlns="http://www.w3.org/2000/svg" width="120" height="120" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" strokeLinecap="round" strokeLinejoin="round" className="text-blue-500"><rect width="18" height="18" x="3" y="3" rx="2" ry="2" /><circle cx="9" cy="9" r="2" /><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" /></svg>
        </div>
        <div className="flex items-center justify-between relative z-10">
          <div>
            <h2 className="text-3xl font-bold flex items-center gap-3 text-slate-900"><span className="w-1.5 h-8 bg-blue-600 rounded-full"></span>创作画廊</h2>
            <p className="text-slate-400 font-medium mt-2 italic shadow-slate-100">记录您的每一次灵感瞬间 (本地路径: {displayPath})</p>
          </div>
          <div className="flex items-center gap-4">
            <button 
              onClick={handleRefresh}
              disabled={isRefreshing}
              className="p-3 bg-slate-50 hover:bg-slate-100 rounded-2xl text-slate-600 border border-slate-200 transition-all flex items-center gap-2 font-bold px-5 active:scale-95 disabled:opacity-50"
            >
              <svg 
                xmlns="http://www.w3.org/2000/svg" 
                width="18" 
                height="18" 
                viewBox="0 0 24 24" 
                fill="none" 
                stroke="currentColor" 
                strokeWidth="2.5" 
                strokeLinecap="round" 
                strokeLinejoin="round"
                className={isRefreshing ? "animate-spin text-blue-600" : ""}
              >
                <path d="M21 12a9 9 0 1 1-6.219-8.56" />
                <path d="M22 10 16 12l3 5 3-7Z" />
              </svg>
              {isRefreshing ? "刷新中..." : "刷新画廊"}
            </button>
            <span className="text-xs font-black text-blue-600 bg-blue-50 px-5 py-2.5 rounded-full border border-blue-100 uppercase tracking-widest">
              {galleryImages.length} ITEMS
            </span>
          </div>
        </div>
      </div>

      {galleryImages.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-32 bg-slate-50/50 rounded-[3rem] border-4 border-dashed border-slate-100 transition-all hover:bg-slate-50">
          <div className="p-8 bg-white rounded-[2.5rem] text-slate-200 mb-8 shadow-2xl border border-slate-100">
            <svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" strokeLinecap="round" strokeLinejoin="round"><rect width="18" height="18" x="3" y="3" rx="2" ry="2" /><circle cx="9" cy="9" r="2" /><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" /></svg>
          </div>
          <p className="text-slate-400 font-black text-xl tracking-tight">暂无任何画作</p>
          <p className="text-slate-300 text-sm mt-2 font-bold uppercase tracking-widest">Start your first creation now</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-10">
          {galleryImages.map((imgObj, i) => {
            const fileName = imgObj.name;
            const imgSrc = imgObj.data;
            return (
              <div key={i} className="group relative bg-white rounded-[2.5rem] overflow-hidden border border-slate-200 shadow-xl hover:shadow-[0_30px_60px_-15px_rgba(0,0,0,0.15)] hover:-translate-y-3 transition-all duration-700">
                <div className="aspect-video relative overflow-hidden">
                  <img 
                    src={imgSrc} 
                    className="w-full h-full object-cover transition-transform duration-1000 group-hover:scale-110" 
                    alt={`Gallery ${i}`} 
                  />
                  <div className="absolute inset-0 bg-slate-900/60 opacity-0 group-hover:opacity-100 transition-all duration-500 flex items-center justify-center gap-3 backdrop-blur-[2px]">
                    <button 
                      onClick={() => onZoom(imgSrc)}
                      className="p-3 bg-white/20 hover:bg-white/40 text-white rounded-2xl shadow-2xl transition-all border border-white/20 font-black text-sm flex flex-col items-center gap-1.5 min-w-[56px]"
                      title="放大预览"
                    >
                      <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" /><line x1="11" y1="8" x2="11" y2="14" /><line x1="8" y1="11" x2="14" y2="11" /></svg>
                      <span>预览</span>
                    </button>
                    <button 
                      onClick={() => {
                        setApplyingName(fileName);
                        sendIpc("set_wallpaper", fileName);
                        setTimeout(() => setApplyingName(null), 3000);
                      }}
                      className="p-3 bg-white rounded-2xl text-slate-900 shadow-2xl hover:scale-105 active:scale-95 transition-all font-black text-sm flex flex-col items-center gap-1.5 min-w-[56px]"
                      title="设为壁纸"
                    >
                      {applyingName === fileName ? (
                        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" className="animate-spin"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>
                      ) : (
                        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><rect width="20" height="14" x="2" y="3" rx="2" /><line x1="8" x2="16" y1="21" y2="21" /><line x1="12" x2="12" y1="17" y2="21" /></svg>
                      )}
                      <span>{applyingName === fileName ? "应用中" : "应用"}</span>
                    </button>
                    <button 
                      onClick={() => setConfirmDeleteName(fileName)}
                      className="p-3 bg-red-500/90 text-white rounded-2xl shadow-2xl hover:scale-105 hover:bg-red-600 active:scale-95 transition-all font-black text-sm flex flex-col items-center gap-1.5 min-w-[56px]"
                      title="从磁盘中彻底移除"
                    >
                      <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M3 6h18" /><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" /><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" /></svg>
                      <span>删除</span>
                    </button>
                  </div>
                </div>
                <div className="p-6 bg-white flex items-center justify-between border-t border-slate-50">
                  <div className="max-w-[70%]">
                    <div className="text-[10px] font-black text-slate-400 uppercase tracking-widest mb-1 truncate">{fileName}</div>
                    <div className="h-1 w-8 bg-blue-600 rounded-full"></div>
                  </div>
                  <div className="p-2 bg-slate-50 rounded-lg text-slate-300">
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><path d="m8 3 4 8 5-5 5 15H2L8 3z"/></svg>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>

    {/* 删除确认 Modal */}
    {confirmDeleteName && (
      <div
        className="fixed inset-0 z-[200] bg-slate-900/60 backdrop-blur-sm flex items-center justify-center animate-in fade-in duration-200"
        onClick={() => setConfirmDeleteName(null)}
      >
        <div
          className="bg-white rounded-[2rem] p-8 shadow-2xl max-w-sm w-full mx-6 animate-in zoom-in-95 duration-200"
          onClick={e => e.stopPropagation()}
        >
          <div className="text-center mb-6">
            <div className="mx-auto w-16 h-16 bg-red-50 rounded-full flex items-center justify-center mb-4">
              <svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-red-500"><path d="M3 6h18" /><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" /><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" /></svg>
            </div>
            <h3 className="text-xl font-black text-slate-900 mb-2">确认删除</h3>
            <p className="text-slate-400 text-sm leading-relaxed">这张壁纸将从磁盘永久移除，<br />无法恢复。</p>
          </div>
          <div className="flex gap-3">
            <button
              onClick={() => setConfirmDeleteName(null)}
              className="flex-1 py-3 bg-slate-100 hover:bg-slate-200 text-slate-700 rounded-2xl font-bold transition-all"
            >
              取消
            </button>
            <button
              onClick={() => {
                sendIpc("delete_image", confirmDeleteName);
                setConfirmDeleteName(null);
              }}
              className="flex-1 py-3 bg-red-500 hover:bg-red-600 text-white rounded-2xl font-bold transition-all"
            >
              确认删除
            </button>
          </div>
        </div>
      </div>
    )}
    </>
  );
};

export default GalleryPage;
