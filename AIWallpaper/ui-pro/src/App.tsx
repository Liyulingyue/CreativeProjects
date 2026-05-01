import { useState, useEffect, useCallback, useRef } from "react";
import Header from "./components/Header";
import CreatePage from "./pages/CreatePage";
import GalleryPage from "./pages/GalleryPage";
import TasksPage from "./pages/TasksPage";
import SettingsPage from "./pages/SettingsPage";
import ImageEditorModal from "./components/ImageEditorModal";
import MessageBox from "./components/MessageBox";

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
  const [uiMode, setUiMode] = useState<'lite' | 'pro'>(() => {
    return (localStorage.getItem('ui-mode') as 'lite' | 'pro') || 'pro';
  });
  const [activeTab, setActiveTab] = useState("create");
  const [editingImageUrl, setEditingImageUrl] = useState("");
  const [prompt, setPrompt] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [peUrl, setPeUrl] = useState("");
  const [peKey, setPeKey] = useState("");
  const [peModel, setPeModel] = useState("");
  const [enableCache, setEnableCache] = useState(true);
  const [cacheLimit, setCacheLimit] = useState(100);
  const [galleryPath, setGalleryPath] = useState("");
  const [imageSize, setImageSize] = useState("auto");
  const [autoRefreshMinutes, setAutoRefreshMinutes] = useState(24 * 60);
  const [autoPrompt, setAutoPrompt] = useState("");
  const [previewUrl, setPreviewUrl] = useState("");
  const [galleryImages, setGalleryImages] = useState<any[]>([]);
  const [galleryPage, setGalleryPage] = useState(0);
  const [galleryTotal, setGalleryTotal] = useState(0);
  const galleryPageSize = 20;
  const [showViewer, setShowViewer] = useState(false);
  const [viewerUrl, setViewerUrl] = useState("");
  const [message, setMessage] = useState("");
  const [messageVisible, setMessageVisible] = useState(false);
  const [isGenerating, setIsGenerating] = useState(false);
  const [isEnhancing, setIsEnhancing] = useState(false);
  const [statusMsg, setStatusMsg] = useState("");
  const [autoStart, setAutoStart] = useState(false);
  const isGeneratingRef = useRef(false);

  useEffect(() => {
    localStorage.setItem('ui-mode', uiMode);
  }, [uiMode]);

  useEffect(() => {
    (window as any).__syncUiMode = (mode: 'lite' | 'pro') => {
      setUiMode(mode);
    };
  }, [setUiMode]);

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
      if (config.ui_mode === "lite" || config.ui_mode === "pro") {
        setUiMode(config.ui_mode);
      }
      setEnableCache(config.enable_cache);
      setCacheLimit(config.cache_limit);
      const rawMinutes = typeof config.auto_refresh_minutes === "number"
        ? config.auto_refresh_minutes
        : ((config.auto_refresh_hours || 0) * 60);
      if (rawMinutes <= 0) {
        setAutoRefreshMinutes(0);
      } else if (rawMinutes < 15) {
        // 允许测试场景使用分钟级(1~14 分钟)
        setAutoRefreshMinutes(Math.max(1, rawMinutes));
      } else {
        const normalized = Math.max(15, Math.round(rawMinutes / 15) * 15);
        setAutoRefreshMinutes(normalized);
      }
      setAutoPrompt(config.auto_prompt || "");
      setGalleryPath(config.gallery_path || "");
      setImageSize(config.image_size || "auto");
      setPeUrl(config.pe_url || "");
      setPeKey(config.pe_key || "");
      setPeModel(config.pe_model || "");
      setAutoStart(!!config.auto_start);
    };

    // 与 Rust 回调签名对齐: onGenerationComplete(success, errorMsg, imagePayload)
    // @ts-ignore
    window.onGenerationComplete = (success: boolean, errorMsg: string, imagePayload: any) => {
      setIsGenerating(false);
      setIsEnhancing(false); // 确保生成完成时也重置优化按钮状态
      if (success && imagePayload?.previewUrl) {
        setStatusMsg(`生成成功！(${imagePayload.size || '未知尺寸'})`);
        if (imagePayload.size && imagePayload.size !== "auto") {
          setImageSize(imagePayload.size);
        }
        setPreviewUrl(imagePayload.previewUrl);
        // 生成成功后自动刷新画廊数据
        sendIpc("get_gallery", { page: 0, page_size: galleryPageSize });
        setTimeout(() => setStatusMsg(""), 3000);
      } else {
        const msg = "生成失败：" + (errorMsg || "未知错误");
        // 如果有统一的 showMessage，可用它显示浮层；否则回落到 statusMsg
        try {
          // @ts-ignore
          if (typeof (window as any).__showMessage === 'function') {
            // pass through to global (if any)
            // @ts-ignore
            (window as any).__showMessage(msg, 5000);
          } else {
            setStatusMsg(msg);
            setTimeout(() => setStatusMsg(""), 5000);
          }
        } catch (e) {
          setStatusMsg(msg);
          setTimeout(() => setStatusMsg(""), 5000);
        }
      }
    };

    // @ts-ignore
    window.onGalleryLoaded = (payload: any) => {
      // 支持分页格式 {items, total, page, page_size}
      if (payload && Array.isArray(payload.items)) {
        setGalleryImages(payload.items);
        setGalleryTotal(payload.total ?? payload.items.length);
        setGalleryPage(payload.page ?? 0);
      } else if (Array.isArray(payload)) {
        // 兼容旧格式
        setGalleryImages(payload);
        setGalleryTotal(payload.length);
        setGalleryPage(0);
      }
    };

    // @ts-ignore
    window.onPromptEnhanced = (enhanced: string) => {
      setIsGenerating(false);
      setIsEnhancing(false); // 确保重置优化按钮状态
      if (enhanced) {
        setPrompt(enhanced);
        setStatusMsg("提示词已优化");
        setTimeout(() => setStatusMsg(""), 3000);
      }
    };

    // 错误处理统一增加状态重置
    // @ts-ignore
    window.onGenerationError = (msg: string) => {
      setIsGenerating(false);
      setIsEnhancing(false);
      setStatusMsg(msg);
      setTimeout(() => setStatusMsg(""), 5000);
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
      // @ts-ignore
      delete window.onPromptEnhanced;
      // @ts-ignore
      delete window.onGenerationError;
    };
  }, [sendIpc]);

  useEffect(() => {
    if (activeTab === "gallery") {
      sendIpc("get_gallery", { page: galleryPage, page_size: galleryPageSize });
    }
  }, [activeTab, galleryPage, sendIpc]);

  useEffect(() => {
    isGeneratingRef.current = isGenerating;
  }, [isGenerating]);



  const triggerGenerate = useCallback((promptText: string, isAuto: boolean = false) => {
    const finalPrompt = promptText.trim();
    if (!finalPrompt || isGeneratingRef.current) return;

    setIsGenerating(true);
    setStatusMsg(isAuto ? "自动刷新触发生成中..." : "正在构思画面...");

    const payload = {
      type: "generate",
      value: finalPrompt,
      size: imageSize
    };
    (window as any).ipc?.postMessage(JSON.stringify(payload));
  }, [imageSize]);

  useEffect(() => {
    if (autoRefreshMinutes <= 0) return;
    if (!apiKey.trim()) return;

    const intervalMs = autoRefreshMinutes * 60 * 1000;
    const timer = window.setInterval(() => {
      if (isGeneratingRef.current) return;
      const plannedPrompt = autoPrompt.trim() || prompts[Math.floor(Math.random() * prompts.length)];
      triggerGenerate(plannedPrompt, true);
    }, intervalMs);

    return () => window.clearInterval(timer);
  }, [autoRefreshMinutes, autoPrompt, apiKey, triggerGenerate]);

  const handleGenerate = () => {
    if (!prompt.trim() || isGenerating) return;
    triggerGenerate(prompt, false);
  };

  const showMessage = (msg: string, duration: number = 3000) => {
    setMessage(msg);
    setMessageVisible(true);
    setTimeout(() => {
      setMessageVisible(false);
      setTimeout(() => setMessage(''), 300);
    }, duration);
  };

  // expose to global so onGenerationComplete can use it if needed
  // (also allows other scripts to call it)
  useEffect(() => {
    // @ts-ignore
    (window as any).__showMessage = showMessage;
    return () => {
      // @ts-ignore
      delete (window as any).__showMessage;
    };
  }, [showMessage]);

  const handleRandomPrompt = () => {
    const random = prompts[Math.floor(Math.random() * prompts.length)];
    setPrompt(random);
  };

  const handleSaveKey = () => {
    sendIpc("save_config", {
      api_key: apiKey,
      ui_mode: uiMode,
      enable_cache: enableCache,
      cache_limit: cacheLimit,
      auto_refresh_minutes: autoRefreshMinutes,
      auto_refresh_hours: Math.floor(autoRefreshMinutes / 60),
      auto_prompt: autoPrompt,
      gallery_path: galleryPath,
      image_size: imageSize,
      pe_url: peUrl,
      pe_key: peKey,
      pe_model: peModel,
      auto_start: autoStart,
    });
    setStatusMsg("配置已同步");
    setTimeout(() => setStatusMsg(""), 2000);
  };

  return (
    <div className="min-h-screen bg-slate-50 flex flex-col font-[Inter,system-ui,sans-serif] text-slate-900 selection:bg-blue-100 selection:text-blue-700">
      <Header activeTab={activeTab} setActiveTab={setActiveTab} sendIpc={sendIpc} uiMode={uiMode} setUiMode={setUiMode} />

      <main className={`flex-1 mx-auto w-full overflow-y-auto custom-scrollbar transition-all duration-300 ${uiMode === 'lite' ? 'p-4 max-w-3xl' : 'p-10 max-w-6xl'}`}>
        {activeTab === "create" && (
          <CreatePage
            uiMode={uiMode}
            prompt={prompt}
            peUrl={peUrl}
            peKey={peKey}
            peModel={peModel}
            showMessage={showMessage}
            setPrompt={setPrompt}
            handleRandomPrompt={handleRandomPrompt}
            handleGenerate={handleGenerate}
            isGenerating={isGenerating}
            isEnhancing={isEnhancing}
            setIsEnhancing={setIsEnhancing}
            statusMsg={statusMsg}
            previewUrl={previewUrl}
            sendIpc={sendIpc}
            setActiveTab={setActiveTab}
            setShowViewer={(v) => {
              if (v) setViewerUrl(previewUrl);
              setShowViewer(v);
            }}
              autoRefreshMinutes={autoRefreshMinutes}
          />
        )}

        {/* global message box */}
        {/* lazy import component below */}
        { /* import at top to ensure included by bundler */ }
        

        {activeTab === "gallery" && (
          <GalleryPage 
            galleryImages={galleryImages} 
            sendIpc={sendIpc} 
            galleryPath={galleryPath} 
            galleryPage={galleryPage}
            galleryTotal={galleryTotal}
            galleryPageSize={galleryPageSize}
            onPageChange={(page) => {
              setGalleryPage(page);
              sendIpc("get_gallery", { page, page_size: galleryPageSize });
            }} 
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
            autoRefreshMinutes={autoRefreshMinutes}
            setAutoRefreshMinutes={setAutoRefreshMinutes}
            autoPrompt={autoPrompt}
            setAutoPrompt={setAutoPrompt}
            handleSaveKey={handleSaveKey}
          />
        )}

        {activeTab === "settings" && (
          <SettingsPage
            uiMode={uiMode}
            setUiMode={setUiMode}
            apiKey={apiKey}
            setApiKey={setApiKey}
            peUrl={peUrl}
            setPeUrl={setPeUrl}
            peKey={peKey}
            setPeKey={setPeKey}
            peModel={peModel}
            setPeModel={setPeModel}
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
                auto_refresh_minutes: autoRefreshMinutes,
                auto_refresh_hours: Math.floor(autoRefreshMinutes / 60),
                auto_prompt: autoPrompt,
                gallery_path: galleryPath,
                image_size: v,
                pe_url: peUrl,
                pe_key: peKey,
                pe_model: peModel,
                auto_start: autoStart,
              });
            }}
            handleSaveKey={handleSaveKey}
            autoStart={autoStart}
            setAutoStart={setAutoStart}
          />
        )}
      </main>

      <MessageBox message={message} visible={messageVisible} />

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
