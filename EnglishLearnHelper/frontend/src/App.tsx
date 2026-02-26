import { Routes, Route } from 'react-router-dom'
import Header from './components/Header'
import Home from './pages/Home'
import Vocab from './pages/Vocab'
import Random from './pages/Random'

function App() {
  return (
    <div className="app">
      <Header />
      <main className="main-content">
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/vocab" element={<Vocab />} />
          <Route path="/random" element={<Random />} />
        </Routes>
      </main>
    </div>
  )
}

export default App
