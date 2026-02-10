import { useState, useEffect } from 'react';

interface FileUploadProps {
  onUploadSuccess: () => void;
  currentPath: string[];
}

export default function FileUpload({ onUploadSuccess, currentPath }: FileUploadProps) {
  const [isDragging, setIsDragging] = useState(false);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const handleWindowDragOver = (e: DragEvent) => {
      e.preventDefault();
      // Only show drag overlay for external files, not internal drag operations
      if (e.dataTransfer?.types.includes('Files')) {
        setIsDragging(true);
      }
    };

    const handleWindowDragLeave = (e: DragEvent) => {
      e.preventDefault();
      // Only set to false if we're actually leaving the window
      if (e.relatedTarget === null) {
        setIsDragging(false);
      }
    };

    const handleWindowDrop = (e: DragEvent) => {
      e.preventDefault();
      setIsDragging(false);
      const files = Array.from(e.dataTransfer?.files || []);
      files.forEach(file => handleUpload(file));
    };

    window.addEventListener('dragover', handleWindowDragOver);
    window.addEventListener('dragleave', handleWindowDragLeave);
    window.addEventListener('drop', handleWindowDrop);

    return () => {
      window.removeEventListener('dragover', handleWindowDragOver);
      window.removeEventListener('dragleave', handleWindowDragLeave);
      window.removeEventListener('drop', handleWindowDrop);
    };
  }, [currentPath]); // Re-bind if these change to ensure handleUpload uses latest

  const handleUpload = async (file: File) => {
    setLoading(true);
    const formData = new FormData();
    const prefix = currentPath.length > 0 ? currentPath.join('/') + '/' : '';
    formData.append('file', file);
    formData.append('target_path', prefix + file.name);
    formData.append('comment', `Uploaded to ${prefix || 'root'}`);

    try {
      const response = await fetch('http://localhost:8000/upload', {
        method: 'POST',
        body: formData,
      });
      if (response.ok) {
        onUploadSuccess();
        // Comment is not reset here automatically because multiple files might be uploading
      }
    } catch (error) {
      console.error('Upload failed:', error);
    } finally {
      setLoading(false);
    }
  };

  return (
    <>
      <div 
        className={`fixed inset-0 z-50 transition-all duration-300 pointer-events-none ${
          isDragging ? 'opacity-100' : 'opacity-0'
        }`}
      >
        <div className="absolute inset-0 bg-indigo-600/40 backdrop-blur-md flex items-center justify-center p-10">
          <div className="w-full max-w-2xl aspect-video border-4 border-dashed border-white rounded-[40px] flex flex-col items-center justify-center text-white scale-95 transition-transform duration-500 animate-in fade-in zoom-in">
            <div className="text-8xl mb-6 animate-bounce">📥</div>
            <h2 className="text-4xl font-black mb-2 uppercase tracking-tighter">释放即刻存入</h2>
            <p className="text-indigo-100 font-bold uppercase tracking-[0.3em] text-sm">
              Current Directory: /{currentPath.join('/')}
            </p>
          </div>
        </div>
      </div>
      
      {loading && (
        <div className="fixed bottom-10 right-10 z-[60] bg-white p-4 rounded-2xl shadow-2xl border border-slate-100 flex items-center space-x-4 animate-in slide-in-from-right">
          <div className="w-5 h-5 border-2 border-indigo-600 border-t-transparent rounded-full animate-spin"></div>
          <span className="text-xs font-black text-slate-700 uppercase tracking-widest">正在上传并保存文件...</span>
        </div>
      )}
    </>
  );
}
