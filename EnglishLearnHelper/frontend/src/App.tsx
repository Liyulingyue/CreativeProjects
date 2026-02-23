import { Routes, Route } from 'react-router-dom'
import Header from './components/Header'
import Home from './pages/Home'
import Vocab from './pages/Vocab'
import Random from './pages/Random'
import OCR from './pages/OCR'

function App() {
  return (
    <div className="app">
      <Header />
      <main className="main-content">
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/vocab" element={<Vocab />} />
          <Route path="/random" element={<Random />} />
          <Route path="/ocr" element={<OCR />} />
        </Routes>
      </main>
    </div>
  )
}

export default App
