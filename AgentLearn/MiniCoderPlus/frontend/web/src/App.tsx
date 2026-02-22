import { BrowserRouter, Routes, Route } from 'react-router-dom';
import Home from './routes/Home';
import Workbench from './routes/Workbench';

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Home />} />
        <Route path="/workbench" element={<Workbench />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
