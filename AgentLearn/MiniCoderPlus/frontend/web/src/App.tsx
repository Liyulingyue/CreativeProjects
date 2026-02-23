import { BrowserRouter, Routes, Route } from 'react-router-dom';
import Home from './routes/Home';
import Workbench from './routes/Workbench';
import FeedbackAnalysis from './routes/FeedbackAnalysis';

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Home />} />
        <Route path="/workbench" element={<Workbench />} />
        <Route path="/feedback-analysis" element={<FeedbackAnalysis />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
