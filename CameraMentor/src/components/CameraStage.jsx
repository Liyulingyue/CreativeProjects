import { Icon } from './TopBar';

export default function CameraStage({ videoRef, overlayRef, ready, error, onStart }) {
  return (
    <div className="camera-stage">
      <video ref={videoRef} autoPlay playsInline muted />
      <canvas ref={overlayRef} />

      {!ready && (
        <div className="placeholder">
          <div className="placeholder-icon">
            <Icon name="camera" size={48} />
          </div>
          <p className="placeholder-text">
            {error ? `相机错误: ${error}` : '需要相机权限才能开始'}
          </p>
          <button className="btn-primary" onClick={onStart}>
            启动相机
          </button>
        </div>
      )}
    </div>
  );
}
