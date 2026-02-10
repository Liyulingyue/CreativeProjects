import { useState } from 'react';

interface FileUploadProps {
  onUploadSuccess: () => void;
  currentPath: string[];
}

export default function FileUpload({ onUploadSuccess, currentPath }: FileUploadProps) {
  const [isDragging, setIsDragging] = useState(false);
  const [loading, setLoading] = useState(false);
  const [comment, setComment] = useState('');

  const handleUpload = async (file: File) => {
    setLoading(true);
    const formData = new FormData();
    const prefix = currentPath.length > 0 ? currentPath.join('/') + '/' : '';
    formData.append('file', file);
    formData.append('target_path', prefix + file.name);
    formData.append('comment', comment || `Uploaded to ${prefix || 'root'}`);

    try {
      const response = await fetch('http://localhost:8000/upload', {
        method: 'POST',
        body: formData,
      });
      if (response.ok) {
        onUploadSuccess();
        setComment('');
      }
    } catch (error) {
      console.error('Upload failed:', error);
    } finally {
      setLoading(false);
    }
  };

  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    const files = Array.from(e.dataTransfer.files);
    files.forEach(file => handleUpload(file));
  };

  return (
    <div 
      onDragOver={(e) => { e.preventDefault(); setIsDragging(true); }}
      onDragLeave={() => setIsDragging(false)}
      onDrop={onDrop}
      className={`fixed inset-0 z-50 transition-all duration-300 pointer-events-none ${
        isDragging ? 'opacity-100' : 'opacity-0'
      }`}
    >
      <div className="absolute inset-0 bg-indigo-600/20 backdrop-blur-md flex items-center justify-center p-10">
        <div className="w-full max-w-2xl aspect-video border-4 border-dashed border-white rounded-[40px] flex flex-col items-center justify-center text-white">
          <div className="text-8xl mb-6 animate-bounce">📥</div>
          <h2 className="text-4xl font-black mb-2">释放即刻存入此目录</h2>
          <p className="text-white/70 font-bold uppercase tracking-widest">Gittly Station Desktop Sync</p>
        </div>
      </div>
    </div>
  );
}
