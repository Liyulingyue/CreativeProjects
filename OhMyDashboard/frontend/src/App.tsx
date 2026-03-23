import { useState } from 'react'
import { SystemMonitor } from './components/SystemMonitor'
import { DockerList } from './components/DockerList'
import { StartupMonitor } from './components/StartupMonitor'
import reactLogo from './assets/react.svg'
import viteLogo from './assets/vite.svg'
import heroImg from './assets/hero.png'
import './App.css'

function App() {
  const [count, setCount] = useState(0)

  return (
    <div className="max-w-7xl mx-auto p-6 bg-white min-h-screen text-slate-800">
      <header className="flex items-center gap-6 mb-12 py-8 border-b border-slate-100">
        <div className="relative w-32 h-32">
          <img src={heroImg} className="absolute inset-0 w-full h-full object-contain" alt="Hero" />
          <img src={reactLogo} className="absolute -bottom-2 -right-2 w-10 h-10 animate-spin-slow" alt="React" />
        </div>
        <div>
          <h1 className="text-4xl font-black bg-gradient-to-r from-blue-600 to-indigo-600 bg-clip-text text-transparent">
            Oh My Dashboard
          </h1>
          <p className="text-slate-500 mt-2 font-medium">智能全栈监控与管理面板</p>
        </div>
      </header>
      
      <main className="grid grid-cols-1 gap-8">
        <section>
          <SystemMonitor />
        </section>
        
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
          <section>
            <DockerList />
          </section>
          <section>
            <StartupMonitor />
          </section>
        </div>
      </main>

      <footer className="mt-20 py-8 border-t border-slate-100 flex flex-col items-center gap-4">
        <div className="flex gap-4">
          <button 
            className="px-6 py-2 bg-slate-900 text-white rounded-full font-bold hover:bg-slate-800 transition-all shadow-lg hover:shadow-indigo-200"
            onClick={() => setCount((c) => c + 1)}
          >
            Session Clicks: {count}
          </button>
        </div>
        <p className="text-slate-400 text-sm">Powered by React + Vite + FastAPI</p>
      </footer>
    </div>
  )
}

export default App

