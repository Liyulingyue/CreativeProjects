import { useEffect, useState } from 'react';

const POSTERS = [
  { image: 'beach', label: '海边漫步' },
  { image: 'mountain', label: '登山徒步' },
  { image: 'culture', label: '人文历史' },
  { image: 'food', label: '美食之旅' },
  { image: 'nightcity', label: '夜市探秘' },
  { image: 'hiking', label: '户外探险' },
  { image: 'ocean', label: '海岛度假' },
  { image: 'forest', label: '森林氧吧' },
  { image: 'sunset', label: '沙漠奇观' },
  { image: 'snow', label: '雪国之旅' },
  { image: 'jinan', label: '泉城济南' },
  { image: 'datong', label: '古都大同' },
];

interface Props {
  onClose: () => void;
  onEnter: () => void;
}

export function Interstitial({ onClose, onEnter }: Props) {
  const [countdown, setCountdown] = useState(3);
  const [poster] = useState(() => POSTERS[Math.floor(Math.random() * POSTERS.length)]);

  useEffect(() => {
    const interval = setInterval(() => {
      setCountdown((prev) => {
        if (prev <= 1) {
          clearInterval(interval);
          onClose();
          return 0;
        }
        return prev - 1;
      });
    }, 1000);
    return () => clearInterval(interval);
  }, [onClose]);

  return (
    <div className="interstitial">
      <button className="interstitial-close" onClick={onClose}>×</button>
      <div className="interstitial-content">
        <img src={`/assets/${poster.image}.png`} alt={poster.label} className="interstitial-image" />
        <div className="interstitial-info">
          <h2 className="interstitial-title">{poster.label}</h2>
          <button className="interstitial-btn" onClick={onEnter}>立即进入</button>
          <p className="interstitial-hint">{countdown}s 后关闭</p>
        </div>
      </div>
    </div>
  );
}
