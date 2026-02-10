import { useState, useEffect, useCallback } from 'react'
import axios from 'axios'
import FileUpload from './components/FileUpload'
import FileExplorer from './components/FileExplorer'
import HistoryPanel from './components/HistoryPanel'
import './App.css'

interface FileItem {
  id: number
  filename: string
  hash: string
  size: number
  upload_time: string
  comment: string
}

function App() {
  const [files, setFiles] = useState<FileItem[]>([])
  const [currentPath, setCurrentPath] = useState<string[]>([])
  const [historyFile, setHistoryFile] = useState<string | null>(null)

  const fetchFiles = useCallback(async () => {
    try {
      const response = await axios.get('http://localhost:8000/files')
      setFiles(response.data)
    } catch (error) {
      console.error('Error fetching files:', error)
    }
  }, [])

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

  const getCurrentContents = () => {
    const prefix = currentPath.length > 0 ? currentPath.join('/') + '/' : ''
    const folders = new Set<string>()
    const items: FileItem[] = []

    files.forEach(file => {
      if (file.filename.startsWith(prefix)) {
        const relativePath = file.filename.slice(prefix.length)
        if (relativePath.includes('/')) {
          folders.add(relativePath.split('/')[0])
        } else if (relativePath.length > 0) {
          items.push(file)
        }
      }
    })

    return {
      subFolders: Array.from(folders).sort(),
      currentFiles: items.sort((a, b) => a.filename.localeCompare(b.filename))
    }
  }

  const { subFolders, currentFiles } = getCurrentContents()

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
              <span className="mt-1">VERSIONS</span>
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
          currentFiles={currentFiles}
          currentPath={currentPath}
          onNavigate={(f) => setCurrentPath([...currentPath, f])}
          onBack={() => setCurrentPath(currentPath.slice(0, -1))}
          onRoot={() => setCurrentPath([])}
          onDownload={handleDownload}
          onViewHistory={setHistoryFile}
          onDelete={handleDelete}
          onMove={handleMove}
          onCreateFolder={(name) => {
            // Virtual folder creation for UI - real creation happens on upload
            setCurrentPath([...currentPath, name]);
          }}
          onUploadClick={() => {
            // Signal upload - for now just informative as full screen drag is active
            alert("请直接拖拽文件到页面任何地方进行上传");
          }}
        />
      </div>

      {historyFile && (
        <HistoryPanel 
          filename={historyFile} 
          onClose={() => setHistoryFile(null)} 
        />
      )}

      {/* System Status Bar */}
      <footer className="h-6 bg-indigo-600 text-[9px] font-black text-white flex items-center px-10 justify-between uppercase tracking-widest shrink-0">
        <div className="flex items-center">
          <span className="w-2 h-2 bg-green-400 rounded-full mr-2 animate-pulse"></span>
          Server Online: http://localhost:8000
        </div>
        <div>MVP v0.2.0 • Local Storage Mode</div>
      </footer>
    </div>
  )
}

export default App
