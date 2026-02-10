import { useState, useEffect, useCallback, useMemo } from 'react'
import { Routes, Route, Navigate, useNavigate, useLocation } from 'react-router-dom'
import axios from 'axios'
import FileUpload from './components/FileUpload'
import FileExplorer from './components/FileExplorer'
import './App.css'

interface FileItem {
  id: number
  filename: string
  hash: string
  size: number
  upload_time: string
  comment: string
}

function AppContent() {
  const [files, setFiles] = useState<FileItem[]>([])
  const [subFolders, setSubFolders] = useState<string[]>([])
  
  const navigate = useNavigate()
  const location = useLocation()

  // Derive currentPath from URL metadata
  const currentPath = useMemo(() => {
    const path = location.pathname.startsWith('/filestation') 
      ? location.pathname.substring('/filestation'.length)
      : ''
    return path.split('/').filter(Boolean)
  }, [location.pathname])

  const fetchFiles = useCallback(async () => {
    try {
      const prefix = currentPath.join('/')
      const response = await axios.get(`http://localhost:8000/files?prefix=${encodeURIComponent(prefix)}`)
      setFiles(response.data.files)
      setSubFolders(response.data.folders)
    } catch (error) {
      console.error('Error fetching files:', error)
    }
  }, [currentPath])

  useEffect(() => {
    fetchFiles()
  }, [fetchFiles])

  const handleDownload = async (fileId: number, filename: string) => {
    try {
      const response = await axios.get(`http://localhost:8000/download/${fileId}`, {
        responseType: 'blob',
      })
      const url = window.URL.createObjectURL(new Blob([response.data]))
      const link = document.createElement('a')
      link.href = url
      link.setAttribute('download', filename.split('/').pop() || filename)
      document.body.appendChild(link)
      link.click()
      link.remove()
    } catch (error) {
      console.error('Error downloading file:', error)
    }
  }

  const handleDelete = async (filename: string) => {
    try {
      await axios.delete(`http://localhost:8000/files?filename=${encodeURIComponent(filename)}`)
      fetchFiles()
    } catch (error) {
      console.error('Error deleting file:', error)
      alert('删除失败')
    }
  }

  const handleMove = async (oldPath: string, newPath: string, isFolder: boolean) => {
    try {
      const formData = new FormData()
      formData.append('old_path', oldPath)
      formData.append('new_path', newPath)
      formData.append('is_folder', String(isFolder))
      
      await axios.post('http://localhost:8000/files/move', formData)
      fetchFiles()
    } catch (error) {
      console.error('Error moving file:', error)
      alert('移动/重命名失败')
    }
  }

  const handleMoveBatch = async (moves: { oldPath: string, newPath: string, isFolder: boolean }[]) => {
    if (moves.length === 0) return;
    try {
      const formData = new FormData()
      moves.forEach(m => {
        formData.append('old_paths', m.oldPath)
        formData.append('new_paths', m.newPath)
        formData.append('is_folders', String(m.isFolder))
      })
      
      await axios.post('http://localhost:8000/files/move-batch', formData)
      fetchFiles()
    } catch (error) {
      console.error('Error batch moving files:', error)
      alert('批量操作失败')
    }
  }

  const handleCreateFolder = async (name: string) => {
    try {
      const fullPath = currentPath.length > 0 ? `${currentPath.join('/')}/${name}` : name
      const formData = new FormData()
      formData.append('path', fullPath)
      await axios.post('http://localhost:8000/files/create-folder', formData)
      fetchFiles()
    } catch (error) {
      console.error('Error creating folder:', error)
      alert('创建文件夹失败')
    }
  }

  const navigateTo = (path: string[]) => {
    navigate(`/filestation/${path.join('/')}`)
  }

  return (
    <div className="h-screen w-screen bg-slate-50 text-slate-900 flex flex-col overflow-hidden font-sans">
      {/* Global Drag Layer */}
      <FileUpload 
        onUploadSuccess={fetchFiles}
        currentPath={currentPath}
      />

      {/* Main UI Container */}
      <div className="flex-1 flex flex-col min-h-0">
        <header className="h-16 bg-white border-b border-slate-100 flex items-center justify-between px-10 shrink-0 z-30 shadow-sm">
          <div className="flex items-center space-x-4">
            <div className="w-10 h-10 bg-indigo-600 rounded-2xl flex items-center justify-center text-xl shadow-lg shadow-indigo-200">
              ⚡
            </div>
            <div>
              <h1 className="text-xl font-black text-slate-800 tracking-tighter uppercase leading-none">
                File<span className="text-indigo-600">Station</span>
              </h1>
              <p className="text-[9px] text-slate-400 font-black uppercase tracking-[0.2em] mt-1">
                Content-Addressed Foundation
              </p>
            </div>
          </div>
          <div className="flex items-center space-x-8 text-[10px] font-black text-slate-400 uppercase tracking-widest">
            <div className="flex flex-col items-end">
              <span className="text-indigo-600 leading-none">{files.length}</span>
              <span className="mt-1">STORAGE NODES</span>
            </div>
            <div className="flex flex-col items-end border-l border-slate-100 pl-8">
              <span className="text-slate-800 leading-none">{new Set(files.map(f => f.filename)).size}</span>
              <span className="mt-1">UNIQUE FILES</span>
            </div>
          </div>
        </header>

        {/* Full Screen Explorer */}
        <FileExplorer 
          subFolders={subFolders}
          currentFiles={files}
          currentPath={currentPath}
          onNavigate={(f) => navigateTo([...currentPath, f])}
          onBreadcrumbClick={(path) => navigateTo(path)}
          onBack={() => navigateTo(currentPath.slice(0, -1))}
          onRoot={() => navigateTo([])}
          onDownload={handleDownload}
          onDelete={handleDelete}
          onMove={handleMove}
          onMoveBatch={handleMoveBatch}
          onCreateFolder={handleCreateFolder}
          onUploadClick={() => {
            alert("请直接拖拽文件到页面任何地方进行上传");
          }}
        />
      </div>

      {/* System Status Bar */}
      <footer className="h-6 bg-indigo-600 text-[9px] font-black text-white flex items-center px-10 justify-between uppercase tracking-widest shrink-0">
        <div className="flex items-center">
          <span className="w-2 h-2 bg-green-400 rounded-full mr-2 animate-pulse"></span>
          Server Online: http://localhost:8000
        </div>
        <div>MVP v0.3.0 • Local Storage Mode</div>
      </footer>
    </div>
  )
}

function App() {
  return (
    <Routes>
      <Route path="/filestation/*" element={<AppContent />} />
      <Route path="/" element={<Navigate to="/filestation/" replace />} />
    </Routes>
  )
}

export default App
