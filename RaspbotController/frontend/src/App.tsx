import { StatusBar } from './components/domains/status';
import { CameraView } from './components/domains/camera';
import { RobotController } from './components/domains/robot';
import './App.css';

function App() {
  return (
    <div className="app">
      <header className="app-header">
        <h1>Raspbot Controller</h1>
      </header>

      <main className="app-main">
        <StatusBar />

        <div className="content-grid">
          <div className="left-panel">
            <CameraView />
          </div>

          <div className="right-panel">
            <RobotController />
          </div>
        </div>
      </main>
    </div>
  );
}

export default App;
