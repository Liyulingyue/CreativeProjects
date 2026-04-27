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
  Undo2,
  Hand,
  Pencil
} from "lucide-react";

interface ImageEditorModalProps {
  imageUrl: string;
  onSave: (base64Data: string, asWallpaper: boolean) => void;
  onCancel: () => void;
  isTabMode?: boolean;
}

const ImageEditorModal: React.FC<ImageEditorModalProps> = ({ imageUrl, onSave, onCancel, isTabMode = false }) => {
  const [mode, setMode] = useState<"draw" | "crop">("draw");
  const [tool, setTool] = useState<"pencil" | "hand">("pencil");
  const [cropTool, setCropTool] = useState<"resize" | "move_box" | "move_img">("resize");
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [isDrawing, setIsDrawing] = useState(false);
  const [isPanning, setIsPanning] = useState(false);
  const [brushSize, setBrushSize] = useState(10);
  const [brushColor, setBrushColor] = useState("#ffffff");
  const [lastPos, setLastPos] = useState({ x: 0, y: 0 });
  
  const [crop, setCrop] = useState({ x: 0, y: 0 });
  const [zoom, setZoom] = useState(1);
  const [croppedAreaPixels, setCroppedAreaPixels] = useState<any>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  
  const [cropBoxRect, setCropBoxRect] = useState<null | { left: number; top: number; width: number; height: number }>(null);
  const [dragging, setDragging] = useState<null | { type: string; startX: number; startY: number; startRect: { left: number; top: number; width: number; height: number } }>(null);
  const [imageOffset, setImageOffset] = useState({ x: 0, y: 0 });
  const [startImageOffset, setStartImageOffset] = useState({ x: 0, y: 0 });

  // 在涂鸦模式时，将图片绘制到 canvas 上以供绘制/保存
  useEffect(() => {
    if (!canvasRef.current || !imageUrl || mode !== 'draw') return;
    const canvas = canvasRef.current;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.src = imageUrl.startsWith('data:') ? imageUrl : `${imageUrl}?t=${Date.now()}`;
    img.onload = () => {
      canvas.width = img.width;
      canvas.height = img.height;
      ctx.drawImage(img, 0, 0);
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      try {
        canvas.style.display = 'block';
        canvas.style.maxWidth = '100%';
        canvas.style.height = 'auto';
      } catch (e) {}
    };
  }, [imageUrl, mode]);

  // 记录容器内的相对位置，而不是屏幕绝对位置，以防滚动或平移干扰
  useEffect(() => {
    if (mode !== "crop") return;
    const img = imageRef.current;
    if (!img) return;
    
    // 初始化：裁剪框居中显示
    const container = document.getElementById('crop-viewport');
    if (container) {
      const crect = container.getBoundingClientRect();
      const size = Math.min(crect.width, crect.height) * 0.6;
      setCropBoxRect({ 
        left: (crect.width - size) / 2, 
        top: (crect.height - size) / 2, 
        width: size, 
        height: size 
      });
    }
  }, [mode]);

  // 拖拽逻辑（移动 + 四角手柄）
  useEffect(() => {
    if (!dragging) return;
    const onMove = (e: MouseEvent) => {
      if (!dragging) return;
      const dx = e.clientX - dragging.startX;
      const dy = e.clientY - dragging.startY;
      const s = dragging.startRect;
      let nr = { ...s };
      const minSize = 30;

      if (dragging.type === 'img') {
        // 移动图片时，cropBoxRect 保持不变（它已经与屏幕对齐了）
        setImageOffset({ 
          x: startImageOffset.x + dx / Math.max(zoom, 0.0001), 
          y: startImageOffset.y + dy / Math.max(zoom, 0.0001) 
        });
        return;
      }
      
      if (dragging.type === 'move') {
        // 移动框体时，图片位置不动，只更新框体在屏幕上的坐标
        nr.left = s.left + dx;
        nr.top = s.top + dy;
      } else if (dragging.type === 'nw') {
        nr.left = s.left + dx;
        nr.top = s.top + dy;
        nr.width = Math.max(minSize, s.width - dx);
        nr.height = Math.max(minSize, s.height - dy);
      } else if (dragging.type === 'ne') {
        nr.top = s.top + dy;
        nr.width = Math.max(minSize, s.width + dx);
        nr.height = Math.max(minSize, s.height - dy);
      } else if (dragging.type === 'sw') {
        nr.left = s.left + dx;
        nr.width = Math.max(minSize, s.width - dx);
        nr.height = Math.max(minSize, s.height + dy);
      } else if (dragging.type === 'se') {
        nr.width = Math.max(minSize, s.width + dx);
        nr.height = Math.max(minSize, s.height + dy);
      }
      setCropBoxRect(nr);
    };
    const onUp = () => setDragging(null);
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [dragging]);

  const getPos = (e: any) => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };
    const rect = canvas.getBoundingClientRect();
    const clientX = e.touches ? e.touches[0].clientX : e.clientX;
    const clientY = e.touches ? e.touches[0].clientY : e.clientY;
    return {
      x: (clientX - rect.left) / zoom * (canvas.width / (rect.width / zoom)),
      y: (clientY - rect.top) / zoom * (canvas.height / (rect.height / zoom))
    };
  };

  const handleMouseDown = (e: React.MouseEvent | React.TouchEvent) => {
    if (mode !== "draw") return;
    if (tool === "hand" || (e as any).button === 1) {
      setIsPanning(true);
      const clientX = (e as any).clientX || (e as any).touches?.[0].clientX;
      const clientY = (e as any).clientY || (e as any).touches?.[0].clientY;
      setLastPos({ x: clientX, y: clientY });
    } else {
      setIsDrawing(true);
      setLastPos(getPos(e));
    }
  };

  const handleMouseMove = (e: React.MouseEvent | React.TouchEvent) => {
    if (mode !== "draw") return;
    if (isPanning) {
      const clientX = (e as any).clientX || (e as any).touches?.[0].clientX;
      const clientY = (e as any).clientY || (e as any).touches?.[0].clientY;
      const dx = clientX - lastPos.x;
      const dy = clientY - lastPos.y;
      if (containerRef.current) {
        containerRef.current.scrollLeft -= dx;
        containerRef.current.scrollTop -= dy;
      }
      setLastPos({ x: clientX, y: clientY });
      return;
    }
    if (!isDrawing || !canvasRef.current) return;
    const ctx = canvasRef.current.getContext("2d");
    if (!ctx) return;
    const currentPos = getPos(e);
    ctx.beginPath();
    ctx.strokeStyle = brushColor;
    ctx.lineWidth = brushSize;
    ctx.moveTo(lastPos.x, lastPos.y);
    ctx.lineTo(currentPos.x, currentPos.y);
    ctx.stroke();
    setLastPos(currentPos);
  };

  const handleMouseUp = () => {
    setIsDrawing(false);
    setIsPanning(false);
  };

  const onCropComplete = useCallback((_setCroppedArea: any, croppedAreaPixels: any) => {
    setCroppedAreaPixels(croppedAreaPixels);
  }, []);

  const handleFinalSave = async (asWallpaper: boolean) => {
    let data;
    if (mode === "draw") {
      data = canvasRef.current?.toDataURL("image/png");
    } else {
      // 使用可视裁剪框进行裁剪（基于 imageRef 与 cropBoxRect）
      const imgEl = imageRef.current;
      if (!imgEl || !cropBoxRect) return;
      const naturalW = (imgEl as HTMLImageElement).naturalWidth;
      const naturalH = (imgEl as HTMLImageElement).naturalHeight;
      const dispRect = imgEl.getBoundingClientRect();
      // 计算裁剪框在图片自然大小坐标系下的位置
      const scaleX = naturalW / dispRect.width;
      const scaleY = naturalH / dispRect.height;
      const sx = (cropBoxRect.left - dispRect.left) * scaleX;
      const sy = (cropBoxRect.top - dispRect.top) * scaleY;
      const sw = cropBoxRect.width * scaleX;
      const sh = cropBoxRect.height * scaleY;
      const canvas = document.createElement("canvas");
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      canvas.width = Math.max(1, Math.floor(sw));
      canvas.height = Math.max(1, Math.floor(sh));
      const image = await new Promise<HTMLImageElement>((res, rej) => {
        const i = new Image();
        i.crossOrigin = "anonymous";
        i.onload = () => res(i);
        i.onerror = rej;
        i.src = imageUrl.startsWith("data:") ? imageUrl : `${imageUrl}?t=${Date.now()}`;
      });
      ctx.drawImage(image, sx, sy, sw, sh, 0, 0, canvas.width, canvas.height);
      data = canvas.toDataURL("image/png");
    }
    if (data) onSave(data, asWallpaper);
    setIsDropdownOpen(false);
  };

  return (
    <div className={`${isTabMode ? "h-full w-full" : "fixed inset-0 z-[1000] bg-slate-900/90 backdrop-blur-md"} flex flex-col p-6 animate-in fade-in duration-300 font-sans text-slate-900`}>
      <style>{`
        .reactEasyCrop_CropArea {
          color: rgba(255, 255, 255, 0.5) !important;
          border: 2px solid #3b82f6 !important;
        }
        .reactEasyCrop_Container {
          cursor: crosshair !important;
        }
      `}</style>
      <div className={`mb-6 ${isTabMode ? "bg-white border-slate-200" : "bg-white/5 border-white/10"} p-4 rounded-3xl border shadow-sm`}>
        <div className="flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <h2 className={`text-xl font-black ${isTabMode ? "text-slate-900" : "text-white"} px-2`}>编辑器</h2>
              <div className="flex bg-slate-100 p-1 rounded-2xl border border-slate-200">
                <button onClick={() => setMode("draw")} className={`flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-bold transition-all ${mode === "draw" ? "bg-white text-blue-600 shadow-sm" : "text-slate-500 hover:text-slate-700"}`}><Undo2 size={16} /> 涂鸦</button>
                <button onClick={() => setMode("crop")} className={`flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-bold transition-all ${mode === "crop" ? "bg-white text-blue-600 shadow-sm" : "text-slate-500 hover:text-slate-700"}`}><CropIcon size={16} /> 裁剪</button>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <button onClick={onCancel} className="px-4 py-2.5 rounded-2xl bg-slate-100 hover:bg-slate-200 text-slate-600 font-bold transition-all flex items-center justify-center text-sm leading-none">取消</button>
              <div className="relative flex items-center">
                <button onClick={() => handleFinalSave(false)} className="px-4 py-2.5 bg-blue-600 text-white rounded-l-2xl font-black shadow-lg shadow-blue-500/20 hover:bg-blue-700 transition-all flex items-center gap-2 border-r border-blue-400/30 text-sm leading-none"><Save size={18} /><span className="ml-1">保存</span></button>
                <button onClick={() => setIsDropdownOpen(!isDropdownOpen)} className="px-4 py-2.5 bg-blue-600 text-white rounded-r-2xl hover:bg-blue-700 transition-all border-l border-blue-800/10 flex items-center justify-center text-sm leading-none"><ChevronDown size={18} className={`transition-transform duration-300 ${isDropdownOpen ? "rotate-180" : ""}`} /></button>
                {isDropdownOpen && (
                  <div className="absolute right-0 top-full mt-2 w-56 bg-white rounded-2xl shadow-2xl border border-slate-100 overflow-hidden z-[1100] animate-in fade-in slide-in-from-top-2">
                    <button onClick={() => handleFinalSave(false)} className="w-full flex items-center gap-3 px-5 py-4 text-left hover:bg-slate-50 transition-colors text-slate-700 font-bold"><Save size={18} className="text-blue-600" /><div><span>仅保存到画廊</span><span className="block text-[10px] text-slate-400 font-normal">不更改当前桌面背景</span></div></button>
                    <button onClick={() => handleFinalSave(true)} className="w-full flex items-center gap-3 px-5 py-4 text-left hover:bg-blue-50 transition-colors text-blue-700 font-bold border-t border-slate-50"><Monitor size={18} className="text-blue-600" /><div><span>保存并设为壁纸</span><span className="block text-[10px] text-blue-400 font-normal">同步更新桌面</span></div></button>
                  </div>
                )}
              </div>
            </div>
          </div>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <div className="flex items-center gap-2">
                <Minus size={14} className="text-slate-400" />
                <input type="range" min="0.5" max="5" step={0.1} value={zoom} onChange={(e) => setZoom(parseFloat(e.target.value))} className="w-32 accent-blue-600" />
                <Plus size={14} className="text-slate-400" />
                <span className="text-[10px] font-bold text-slate-400 w-8">{(zoom * 100).toFixed(0)}%</span>
              </div>
              <button onClick={() => {setZoom(1); setCrop({x:0, y:0})}} className="p-2 hover:bg-slate-100 rounded-lg text-slate-400 hover:text-blue-600 transition-colors" title="重置缩放"><RotateCcw size={18} /></button>
              <div className="h-6 w-px bg-slate-200 mx-2" />
              {mode === "draw" && (
                <div className="flex items-center gap-4">
                  <div className="flex bg-slate-100 p-1 rounded-xl border border-slate-200">
                    <button onClick={() => setTool("pencil")} className={`p-2 rounded-lg transition-all ${tool === "pencil" ? "bg-white text-blue-600 shadow-sm" : "text-slate-500"}`}><Pencil size={16} /></button>
                    <button onClick={() => setTool("hand")} className={`p-2 rounded-lg transition-all ${tool === "hand" ? "bg-white text-blue-600 shadow-sm" : "text-slate-500"}`}><Hand size={16} /></button>
                  </div>
                  <div className="flex items-center gap-3"><span className="text-[10px] font-black text-slate-400 uppercase tracking-widest">颜色</span><input type="color" value={brushColor} onChange={(e) => setBrushColor(e.target.value)} className="w-8 h-8 rounded-lg cursor-pointer border-none bg-transparent" /></div>
                  <div className="flex items-center gap-3"><span className="text-[10px] font-black text-slate-400 uppercase tracking-widest">尺寸</span><input type="range" min="1" max="50" value={brushSize} onChange={(e) => setBrushSize(parseInt(e.target.value))} className="w-24 accent-blue-600" /><span className="text-xs font-bold text-slate-600 w-4">{brushSize}</span></div>
                </div>
              )}
              {mode === "crop" && (
                <div className="flex items-center gap-4">
                  <div className="flex bg-slate-100 p-1 rounded-xl border border-slate-200">
                    <button 
                      onClick={() => setCropTool("resize")} 
                      className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg transition-all text-[11px] font-bold ${cropTool === "resize" ? "bg-white text-blue-600 shadow-sm" : "text-slate-500 hover:text-slate-700"}`}
                      title="调整边框大小"
                    >
                      <Plus size={14} /> 调边框
                    </button>
                    <button 
                      onClick={() => setCropTool("move_box")} 
                      className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg transition-all text-[11px] font-bold ${cropTool === "move_box" ? "bg-white text-blue-600 shadow-sm" : "text-slate-500 hover:text-slate-700"}`}
                      title="移动裁剪框"
                    >
                      <CropIcon size={14} /> 移框
                    </button>
                    <button 
                      onClick={() => setCropTool("move_img")} 
                      className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg transition-all text-[11px] font-bold ${cropTool === "move_img" ? "bg-white text-blue-600 shadow-sm" : "text-slate-500 hover:text-slate-700"}`}
                      title="平移图片内容"
                    >
                      <Hand size={14} /> 移图
                    </button>
                  </div>
                  <div className="h-6 w-px bg-slate-200 mx-1" />
                </div>
              )}
            </div>
            <div className="text-[10px] text-slate-300 font-bold uppercase tracking-widest">
              {mode === "draw" ? (tool === "pencil" ? "正在绘制" : "正在平移") : (
                cropTool === "resize" ? "调整边框" : (cropTool === "move_box" ? "移动裁剪框" : "移动图片")
              )}
            </div>
          </div>
        </div>
      </div>
      <div className={`flex-1 relative rounded-[2.5rem] border ${isTabMode ? "bg-slate-50 border-slate-200" : "bg-slate-800/50 border-white/5"} overflow-hidden shadow-inner`}>
        {mode === "draw" ? (
          <div ref={containerRef} className="absolute inset-0 overflow-auto bg-slate-800/10">
            <div className="min-h-full min-w-full flex items-center justify-center p-[200px]">
              <canvas ref={canvasRef} onMouseDown={handleMouseDown} onMouseMove={handleMouseMove} onMouseUp={handleMouseUp} onMouseLeave={handleMouseUp} onTouchStart={handleMouseDown} onTouchMove={handleMouseMove} onTouchEnd={handleMouseUp} className={`shadow-2xl rounded-lg bg-white ${tool === "hand" ? "cursor-grab active:cursor-grabbing" : "cursor-crosshair"}`} style={{ transform: `scale(${zoom})`, transformOrigin: "center center" }} />
            </div>
          </div>
        ) : (
          <div className="absolute inset-0 bg-slate-900 overflow-hidden flex items-center justify-center" id="crop-viewport">
            {/* 1. 图片层 */}
            <img
              ref={imageRef}
              src={imageUrl}
              crossOrigin="anonymous"
              alt="编辑图片"
              draggable={false}
              onDragStart={(e) => e.preventDefault()}
              onMouseDown={(ev) => {
                if (mode === 'crop' && (cropTool === 'move_img' || ev.button === 1)) {
                  ev.preventDefault();
                  setStartImageOffset(imageOffset);
                  setDragging({ type: 'img', startX: ev.clientX, startY: ev.clientY, startRect: cropBoxRect! });
                }
              }}
              style={{ 
                display: 'block', 
                transform: `translate(${imageOffset.x}px, ${imageOffset.y}px) scale(${zoom})`, 
                transformOrigin: 'center center', 
                maxWidth: 'none',
                position: 'absolute',
                pointerEvents: cropTool === 'move_img' ? 'auto' : 'none',
                zIndex: 5
              }}
            />

            {/* 2. 移图模式下的全屏覆盖层 (确保点哪儿都能拽图) */}
            {mode === 'crop' && cropTool === 'move_img' && (
              <div 
                className="absolute inset-0 z-10 cursor-grab active:cursor-grabbing"
                onMouseDown={(ev) => {
                  ev.preventDefault();
                  setStartImageOffset(imageOffset);
                  setDragging({ type: 'img', startX: ev.clientX, startY: ev.clientY, startRect: cropBoxRect! });
                }}
              />
            )}

            {/* 3. 裁剪框层 (改为 absolute 以容器为基准) */}
            {cropBoxRect && (
              <div
                className="absolute shadow-[0_0_0_9999px_rgba(0,0,0,0.5)]"
                style={{ 
                  left: cropBoxRect.left, 
                  top: cropBoxRect.top, 
                  width: cropBoxRect.width, 
                  height: cropBoxRect.height, 
                  border: `2px ${cropTool === 'move_box' ? 'solid' : 'dashed'} #3b82f6`, 
                  boxSizing: 'border-box', 
                  zIndex: 20,
                  cursor: cropTool === 'move_box' ? 'move' : 'default',
                  pointerEvents: cropTool === 'move_img' ? 'none' : 'auto'
                }}
                onMouseDown={(ev) => {
                  if (cropTool !== 'move_box') return;
                  ev.preventDefault();
                  setDragging({ type: 'move', startX: ev.clientX, startY: ev.clientY, startRect: cropBoxRect! });
                }}
              >
                {/* 手柄 */}
                {cropTool === 'resize' && (
                  <>
                    {[
                      { t: 'nw', s: { left: -6, top: -6, cursor: 'nwse-resize' } },
                      { t: 'ne', s: { right: -6, top: -6, cursor: 'nesw-resize' } },
                      { t: 'sw', s: { left: -6, bottom: -6, cursor: 'nesw-resize' } },
                      { t: 'se', s: { right: -6, bottom: -6, cursor: 'nwse-resize' } },
                    ].map(h => (
                      <div
                        key={h.t}
                        onMouseDown={(e) => { e.stopPropagation(); e.preventDefault(); setDragging({ type: h.t, startX: e.clientX, startY: e.clientY, startRect: cropBoxRect! }); }}
                        className="absolute w-3 h-3 bg-blue-500 border-2 border-white rounded-sm"
                        style={h.s}
                      />
                    ))}
                  </>
                )}
                
                {/* 网格 */}
                <div className="absolute inset-0 pointer-events-none opacity-30">
                  <div className="absolute top-1/3 w-full h-px bg-blue-300" />
                  <div className="absolute top-2/3 w-full h-px bg-blue-300" />
                  <div className="absolute left-1/3 h-full w-px bg-blue-300" />
                  <div className="absolute left-2/3 h-full w-px bg-blue-300" />
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};
export default ImageEditorModal;
