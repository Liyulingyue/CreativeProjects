import { useState, useEffect, useCallback } from "react";
import Header from "./components/Header";
import CreatePage from "./pages/CreatePage";
import GalleryPage from "./pages/GalleryPage";
import TasksPage from "./pages/TasksPage";
import SettingsPage from "./pages/SettingsPage";
import ImageEditorModal from "./components/ImageEditorModal";

const prompts = [
  "Cyberpunk city at night, neon lights, rain on pavement, cinematic lighting, 8k resolution",
  "Zen garden with cherry blossoms, soft morning sunlight, peaceful atmosphere, high detail",
  "Astronaut floating in deep space, colorful nebula in background, hyper-realistic, 4k",
  "Minimalist mountain landscape, sunset, smooth gradients, vector art style",
  "Underwater kingdom, glowing jellyfish, coral reefs, ethereal blue light, fantasy style",
  "Steampunk airship flying through golden clouds at sunset, highly detailed",
  "Ancient forest with glowing mushrooms, magical fireflies, mystery atmosphere",
  "Futuristic interior design, sleek white furniture, large windows looking into a green garden"
];

function App() {
  const [activeTab, setActiveTab] = useState("create");
  const [editingImageUrl, setEditingImageUrl] = useState("");
  const [prompt, setPrompt] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [isGenerating, setIsGenerating] = useState(false);
  const [statusMsg, setStatusMsg] = useState("");
  const [previewUrl, setPreviewUrl] = useState("");
  const [enableCache, setEnableCache] = useState(true);
  const [cacheLimit, setCacheLimit] = useState(50);
  const [autoRefreshHours, setAutoRefreshHours] = useState(0);
  const [autoPrompt, setAutoPrompt] = useState("");
  const [galleryPath, setGalleryPath] = useState("");
  const [imageSize, setImageSize] = useState("auto");
  const [galleryImages, setGalleryImages] = useState<any[]>([]);
  const [showViewer, setShowViewer] = useState(false);
  const [viewerUrl, setViewerUrl] = useState("");

  const sendIpc = useCallback((cmd: string, arg?: any) => {
    const value = arg === undefined ? "" : (typeof arg === "string" ? arg : JSON.stringify(arg));
    (window as any).ipc?.postMessage(JSON.stringify({ type: cmd, value }));
  }, []);

  useEffect(() => {
    // 与 Lite 一致，发送 ready 触发 Rust 的 AppEvent::Ready，后者会回调 onConfigLoaded
    sendIpc("ready");
    // @ts-ignore
    window.onConfigLoaded = (config: any) => {
      console.log("收到后端配置:", config);
      if (config.api_key) setApiKey(config.api_key);
      setEnableCache(config.enable_cache);
      setCacheLimit(config.cache_limit);
      setAutoRefreshHours(config.auto_refresh_hours);
      setAutoPrompt(config.auto_prompt || "");
      setGalleryPath(config.gallery_path || "");
      setImageSize(config.image_size || "auto");
    };

    // 与 Rust 回调签名对齐: onGenerationComplete(success, errorMsg, imagePayload)
    // @ts-ignore
    window.onGenerationComplete = (success: boolean, errorMsg: string, imagePayload: any) => {
      setIsGenerating(false);
      if (success && imagePayload?.previewUrl) {
        setStatusMsg(`生成成功！(${imagePayload.size || '未知尺寸'})`);
        if (imagePayload.size && imagePayload.size !== "auto") {
          setImageSize(imagePayload.size);
        }
        setPreviewUrl(imagePayload.previewUrl);
        // 生成成功后自动刷新画廊数据
        sendIpc("get_gallery");
        setTimeout(() => setStatusMsg(""), 3000);
      } else {
        setStatusMsg("生成失败：" + (errorMsg || "未知错误"));
        setTimeout(() => setStatusMsg(""), 5000);
      }
    };

    // @ts-ignore
    window.onGalleryLoaded = (images: any[]) => {
      setGalleryImages(images);
    };

    // @ts-ignore
    window.onImageSaved = (path: string) => {
      setStatusMsg("✅ 成功存入画廊并应用为壁纸");
      sendIpc("get_gallery");
      setActiveTab("gallery");
      setTimeout(() => setStatusMsg(""), 5000);
    };

    return () => {
      // @ts-ignore
      delete window.onConfigLoaded;
      // @ts-ignore
      delete window.onGenerationComplete;
      // @ts-ignore
      delete window.onGalleryLoaded;
      // @ts-ignore
      delete window.onImageSaved;
    };
  }, [sendIpc]);

  useEffect(() => {
    if (activeTab === "gallery") {
      sendIpc("get_gallery");
    }
  }, [activeTab, sendIpc]);

  const handleGenerate = () => {
    if (!prompt.trim() || isGenerating) return;
    setIsGenerating(true);
    setStatusMsg("正在构思画面...");
    
    // 发送生成指令，同时携带当前选定的 imageSize (可能是 auto 或具体分辨率)
    const payload = {
      type: "generate",
      value: prompt,
      size: imageSize
    };
    (window as any).ipc?.postMessage(JSON.stringify(payload));
  };

  const handleRandomPrompt = () => {
    const random = prompts[Math.floor(Math.random() * prompts.length)];
    setPrompt(random);
  };

  const handleSaveKey = () => {
    sendIpc("save_config", {
      api_key: apiKey,
      enable_cache: enableCache,
      cache_limit: cacheLimit,
      auto_refresh_hours: autoRefreshHours,
      auto_prompt: autoPrompt,
      gallery_path: galleryPath,
      image_size: imageSize
    });
    setStatusMsg("配置已同步");
    setTimeout(() => setStatusMsg(""), 2000);
  };

  return (
    <div className="min-h-screen bg-slate-50 flex flex-col font-[Inter,system-ui,sans-serif] text-slate-900 selection:bg-blue-100 selection:text-blue-700">
      <Header activeTab={activeTab} setActiveTab={setActiveTab} sendIpc={sendIpc} />

      <main className="flex-1 p-10 max-w-6xl mx-auto w-full overflow-y-auto custom-scrollbar">
        {activeTab === "create" && (
          <CreatePage
            prompt={prompt}
            setPrompt={setPrompt}
            handleRandomPrompt={handleRandomPrompt}
            handleGenerate={handleGenerate}
            isGenerating={isGenerating}
            statusMsg={statusMsg}
            previewUrl={previewUrl}
            sendIpc={sendIpc}
            setActiveTab={setActiveTab}
            setShowViewer={(v) => {
              if (v) setViewerUrl(previewUrl);
              setShowViewer(v);
            }}
            autoRefreshHours={autoRefreshHours}
          />
        )}

        {activeTab === "gallery" && (
          <GalleryPage 
            galleryImages={galleryImages} 
            sendIpc={sendIpc} 
            galleryPath={galleryPath} 
            onZoom={(url) => {
              setViewerUrl(url);
              setShowViewer(true);
            }}
            onEdit={(url) => {
                setEditingImageUrl(url);
                setActiveTab("edit");
            }}
          />
        )}

        {activeTab === "edit" && (
            <div className="h-[calc(100vh-200px)] relative overflow-hidden rounded-3xl border border-slate-200 bg-white">
                {editingImageUrl ? (
                    <ImageEditorModal 
                        imageUrl={editingImageUrl}
                        isTabMode={true}
                        onSave={(data, asWallpaper) => {
                            sendIpc("save_edited_image", { data, set_as_wallpaper: asWallpaper });
                            setStatusMsg(asWallpaper ? "正在保存并更新壁纸..." : "正在保存到画廊...");
                        }}
                        onCancel={() => setActiveTab("gallery")}
                    />
                ) : (
                    <div className="flex flex-col items-center justify-center h-full text-slate-400 space-y-4">
                        <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
                        <p className="font-medium text-lg">请先在画廊中选择一张图片进行编辑</p>
                        <button 
                            onClick={() => setActiveTab("gallery")}
                            className="px-6 py-2 bg-blue-600 text-white rounded-xl hover:bg-blue-700 transition-colors shadow-lg shadow-blue-500/20"
                        >
                            去画廊看看
                        </button>
                    </div>
                )}
            </div>
        )}

        {activeTab === "tasks" && (
          <TasksPage
            autoRefreshHours={autoRefreshHours}
            setAutoRefreshHours={setAutoRefreshHours}
            autoPrompt={autoPrompt}
            setAutoPrompt={setAutoPrompt}
            handleSaveKey={handleSaveKey}
          />
        )}

        {activeTab === "settings" && (
          <SettingsPage
            apiKey={apiKey}
            setApiKey={setApiKey}
            enableCache={enableCache}
            setEnableCache={setEnableCache}
            cacheLimit={cacheLimit}
            setCacheLimit={setCacheLimit}
            galleryPath={galleryPath}
            setGalleryPath={setGalleryPath}
            imageSize={imageSize}
            setImageSize={(v) => {
              setImageSize(v);
              sendIpc("save_config", {
                api_key: apiKey,
                enable_cache: enableCache,
                cache_limit: cacheLimit,
                auto_refresh_hours: autoRefreshHours,
                auto_prompt: autoPrompt,
                gallery_path: galleryPath,
                image_size: v
              });
            }}
            handleSaveKey={handleSaveKey}
          />
        )}
      </main>

      {/* 图片查看器 */}
      {showViewer && (
        <div 
          className="fixed inset-0 z-[100] bg-slate-900/90 backdrop-blur-2xl flex items-center justify-center p-10 animate-in fade-in zoom-in-95 duration-300"
          onClick={() => setShowViewer(false)}
        >
          <div className="relative max-w-full max-h-full group" onClick={e => e.stopPropagation()}>
            <img 
              src={viewerUrl} 
              className="max-w-full max-h-[90vh] rounded-[2rem] shadow-2xl border-4 border-white/20 object-contain" 
              alt="Wallpaper Full" 
            />
            <button 
              onClick={() => setShowViewer(false)}
              className="absolute -top-4 -right-4 p-3 bg-white text-slate-900 rounded-full hover:scale-110 active:scale-95 transition-all shadow-xl z-[110]"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><path d="M18 6 6 18" /><path d="m6 6 12 12" /></svg>
            </button>
            <div className="absolute bottom-6 left-1/2 -translate-x-1/2 bg-white/10 backdrop-blur-md border border-white/20 px-6 py-3 rounded-full opacity-0 group-hover:opacity-100 transition-opacity">
               <p className="text-white text-xs font-bold tracking-widest uppercase">PRO Masterpiece View</p>
            </div>
          </div>
        </div>
      )}

      <footer className="px-10 py-6 border-t border-slate-100 text-[10px] text-slate-400 font-bold uppercase tracking-widest flex justify-between items-center">
        <div className="flex gap-6">
          <span>Engine: Tao/Wry</span>
          <span>UI: Tailwind 4 / React</span>
        </div>
        <div className="flex items-center gap-1.5 bg-blue-50 text-blue-600 px-3 py-1 rounded-full border border-blue-100">
          <div className="w-1.5 h-1.5 bg-blue-600 rounded-full animate-pulse"></div>
          Powered by AIWallpaper
        </div>
      </footer>
    </div>
  );
}

export default App;
