import React, { useRef, useEffect, useState, useCallback } from "react";
import Cropper from "react-easy-crop";
import { 
  ChevronDown, 
  Save, 
  Monitor, 
  RotateCcw, 
  Crop as CropIcon, 
  Minus, 
  Plus,
  Undo2
} from "lucide-react";

interface ImageEditorModalProps {
  imageUrl: string;
  onSave: (base64Data: string, asWallpaper: boolean) => void;
  onCancel: () => void;
  isTabMode?: boolean;
}

const ImageEditorModal: React.FC<ImageEditorModalProps> = ({ imageUrl, onSave, onCancel, isTabMode = false }) => {
  const [mode, setMode] = useState<"draw" | "crop">("draw");
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [isDrawing, setIsDrawing] = useState(false);
  const [brushSize, setBrushSize] = useState(10);
  const [brushColor, setBrushColor] = useState("#ffffff");
  const [lastPos, setLastPos] = useState({ x: 0, y: 0 });
  
  const [crop, setCrop] = useState({ x: 0, y: 0 });
  const [zoom, setZoom] = useState(1);
  const [croppedAreaPixels, setCroppedAreaPixels] = useState<any>(null);

  useEffect(() => {
    if (!canvasRef.current || !imageUrl || mode !== "draw") return;
    const canvas = canvasRef.current;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const img = new Image();
    img.crossOrigin = "anonymous";
    img.src = imageUrl.startsWith("data:") ? imageUrl : (imageUrl.includes("?") ? imageUrl : `${imageUrl}?t=${Date.now()}`);
    
    img.onload = () => {
      canvas.width = img.width;
      canvas.height = img.height;
      ctx.drawImage(img, 0, 0);
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
    };
  }, [imageUrl, mode]);

  const getPos = (e: React.MouseEvent | React.TouchEvent) => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };
    const rect = canvas.getBoundingClientRect();
    let clientX, clientY;
    if ("touches" in e) {
      clientX = e.touches[0].clientX;
      clientY = e.touches[0].clientY;
    } else {
      clientX = (e as React.MouseEvent).clientX;
      clientY = (e as React.MouseEvent).clientY;
    }
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    return {
      x: (clientX - rect.left) * scaleX,
      y: (clientY - rect.top) * scaleY
    };
  };

  const startDrawing = (e: React.MouseEvent | React.TouchEvent) => {
    if (mode !== "draw") return;
    setIsDrawing(true);
    setLastPos(getPos(e));
  };

  const draw = (e: React.MouseEvent | React.TouchEvent) => {
    if (!isDrawing || !canvasRef.current || mode !== "draw") return;
    const ctx = canvasRef.current.getContext("2d");
    if (!ctx) return;
    const currentPos = getPos(e);
    ctx.beginPath();
    ctx.strokeStyle = brushColor;
    ctx.lineWidth = brushSize;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.moveTo(lastPos.x, lastPos.y);
    ctx.lineTo(currentPos.x, currentPos.y);
    ctx.stroke();
    setLastPos(currentPos);
  };

  const stopDrawing = () => setIsDrawing(false);

  const onCropComplete = useCallback((_setCroppedArea: any, croppedAreaPixels: any) => {
    setCroppedAreaPixels(croppedAreaPixels);
  }, []);

  const createImage = (url: string): Promise<HTMLImageElement> =>
    new Promise((resolve, reject) => {
      const image = new Image();
      image.addEventListener("load", () => resolve(image));
      image.addEventListener("error", (error) => reject(error));
      image.setAttribute("crossOrigin", "anonymous");
      image.src = url;
    });

  const getCroppedImg = async () => {
    try {
      const image = await createImage(imageUrl);
      const canvas = document.createElement("canvas");
      const ctx = canvas.getContext("2d");
      if (!ctx) return null;
      canvas.width = croppedAreaPixels.width;
      canvas.height = croppedAreaPixels.height;
      ctx.drawImage(
        image,
        croppedAreaPixels.x,
        croppedAreaPixels.y,
        croppedAreaPixels.width,
        croppedAreaPixels.height,
        0,
        0,
        croppedAreaPixels.width,
        croppedAreaPixels.height
      );
      return canvas.toDataURL("image/png");
    } catch (e) {
      console.error(e);
      return null;
    }
  };

  const handleFinalSave = async (asWallpaper: boolean) => {
    let data;
    if (mode === "draw") {
      data = canvasRef.current?.toDataURL("image/png");
    } else {
      data = await getCroppedImg();
    }
    if (data) {
      onSave(data, asWallpaper);
    }
    setIsDropdownOpen(false);
  };

  return (
    <div className={`${isTabMode ? "h-full w-full" : "fixed inset-0 z-[1000] bg-slate-900/90 backdrop-blur-md"} flex flex-col p-6 animate-in fade-in duration-300 font-sans text-slate-900`}>
      <div className={`flex items-center justify-between mb-6 ${isTabMode ? "bg-white border-slate-200" : "bg-white/5 border-white/10"} p-4 rounded-3xl border shadow-sm`}>
        <div className="flex items-center gap-4">
          <h2 className={`text-xl font-black ${isTabMode ? "text-slate-900" : "text-white"} px-2`}>编辑器</h2>
          <div className="flex bg-slate-100 p-1 rounded-2xl border border-slate-200">
            <button onClick={() => setMode("draw")} className={`flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-bold transition-all ${mode === "draw" ? "bg-white text-blue-600 shadow-sm" : "text-slate-500 hover:text-slate-700"}`}><Undo2 size={16} /> 涂鸦</button>
            <button onClick={() => setMode("crop")} className={`flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-bold transition-all ${mode === "crop" ? "bg-white text-blue-600 shadow-sm" : "text-slate-500 hover:text-slate-700"}`}><CropIcon size={16} /> 裁剪/缩放</button>
          </div>
          <div className="h-6 w-px bg-slate-200 mx-2" />
          {mode === "draw" ? (
            <div className="flex items-center gap-6">
              <div className="flex items-center gap-3"><span className="text-[10px] font-black text-slate-400 uppercase tracking-widest">颜色</span><input type="color" value={brushColor} onChange={(e) => setBrushColor(e.target.value)} className="w-8 h-8 rounded-lg cursor-pointer border-none bg-transparent" /></div>
              <div className="flex items-center gap-3"><span className="text-[10px] font-black text-slate-400 uppercase tracking-widest">尺寸</span><input type="range" min="1" max="50" value={brushSize} onChange={(e) => setBrushSize(parseInt(e.target.value))} className="w-24 accent-blue-600" /><span className="text-xs font-bold text-slate-600 w-4">{brushSize}</span></div>
            </div>
          ) : (
            <div className="flex items-center gap-6">
              <div className="flex items-center gap-3"><Minus size={14} className="text-slate-400" /><input type="range" min="1" max="3" step={0.1} value={zoom} onChange={(e) => setZoom(parseFloat(e.target.value))} className="w-32 accent-blue-600" /><Plus size={14} className="text-slate-400" /></div>
              <button onClick={() => {setZoom(1); setCrop({x:0, y:0})}} className="p-2 hover:bg-slate-100 rounded-lg text-slate-400 hover:text-blue-600 transition-colors"><RotateCcw size={18} /></button>
            </div>
          )}
        </div>
        <div className="flex items-center gap-3">
          <button onClick={onCancel} className="px-6 py-2.5 rounded-2xl bg-slate-100 hover:bg-slate-200 text-slate-600 font-bold transition-all active:scale-95">取消</button>
          <div className="relative flex items-center">
            <button onClick={() => handleFinalSave(true)} className="pl-8 pr-4 py-2.5 bg-blue-600 text-white rounded-l-2xl font-black shadow-lg shadow-blue-500/20 hover:bg-blue-700 transition-all flex items-center gap-2 border-r border-blue-400/30"><Monitor size={18} /> 保存并设为壁纸</button>
            <button onClick={() => setIsDropdownOpen(!isDropdownOpen)} className="px-3 py-2.5 bg-blue-600 text-white rounded-r-2xl hover:bg-blue-700 transition-all border-l border-blue-800/10"><ChevronDown size={18} className={`transition-transform duration-300 ${isDropdownOpen ? "rotate-180" : ""}`} /></button>
            {isDropdownOpen && (
              <div className="absolute right-0 top-full mt-2 w-56 bg-white rounded-2xl shadow-2xl border border-slate-100 overflow-hidden z-[1100] animate-in fade-in slide-in-from-top-2">
                <button onClick={() => handleFinalSave(false)} className="w-full flex items-center gap-3 px-5 py-4 text-left hover:bg-slate-50 transition-colors text-slate-700 font-bold"><Save size={18} className="text-blue-600" /><div><span>仅保存到画廊</span><span className="block text-[10px] text-slate-400 font-normal">不更改当前桌面背景</span></div></button>
                <button onClick={() => handleFinalSave(true)} className="w-full flex items-center gap-3 px-5 py-4 text-left hover:bg-blue-50 transition-colors text-blue-700 font-bold border-t border-slate-50"><Monitor size={18} className="text-blue-600" /><div><span>保存并设为壁纸</span><span className="block text-[10px] text-blue-400 font-normal">同步更新桌面</span></div></button>
              </div>
            )}
          </div>
        </div>
      </div>
      <div className={`flex-1 relative rounded-[2.5rem] border ${isTabMode ? "bg-slate-50 border-slate-200" : "bg-slate-800/50 border-white/5"} overflow-hidden shadow-inner`}>
        {mode === "draw" ? (
          <div className="absolute inset-0 flex items-center justify-center p-12">
            <canvas ref={canvasRef} onMouseDown={startDrawing} onMouseMove={draw} onMouseUp={stopDrawing} onMouseLeave={stopDrawing} onTouchStart={startDrawing} onTouchMove={draw} onTouchEnd={stopDrawing} className="max-w-full max-h-full object-contain shadow-2xl rounded-lg cursor-crosshair bg-white ring-8 ring-white/50" />
          </div>
        ) : (
          <div className="absolute inset-0 bg-slate-900">
            <Cropper image={imageUrl} crop={crop} zoom={zoom} aspect={undefined} onCropChange={setCrop} onCropComplete={onCropComplete} onZoomChange={setZoom} />
          </div>
        )}
        <div className="absolute bottom-6 left-1/2 -translate-x-1/2 px-5 py-2.5 rounded-full border border-slate-200/50 bg-white/80 backdrop-blur-md text-slate-500 shadow-xl flex items-center gap-3 pointer-events-none">
          <div className="w-1.5 h-1.5 bg-blue-500 rounded-full animate-pulse" />
          <span className="text-[10px] font-bold uppercase tracking-widest leading-none">{mode === "draw" ? "当前模式：自由涂鸦" : "当前模式：裁剪、缩放与移动"}</span>
        </div>
      </div>
    </div>
  );
};
export default ImageEditorModal;
