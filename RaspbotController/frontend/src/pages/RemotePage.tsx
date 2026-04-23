import { StatusBar } from '../components/domains/status';
import { CameraView } from '../components/domains/camera';
import { RobotController } from '../components/domains/robot';
import { ServoControl } from '../components/domains/servo';
import { BuzzerControl } from '../components/domains/buzzer';
import { DistanceSensor } from '../components/domains/distance';
import { LEDControl } from '../components/domains/led';
import '../App.css';

export const RemotePage = ({ showCamera = true }: { showCamera?: boolean }) => {
  return (
    <div className="page">
      <div className="page-header">
        <h2>Remote Control</h2>
        <p className="page-desc">Control your robot remotely</p>
      </div>

      <StatusBar />

      <div className="content-grid">
        <div className="left-panel">
          {showCamera && <CameraView />}
          <ServoControl />
          <DistanceSensor />
        </div>

        <div className="right-panel">
          <RobotController />
          <BuzzerControl />
          <LEDControl />
        </div>
      </div>
    </div>
  );
};
