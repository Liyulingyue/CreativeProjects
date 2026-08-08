interface BreakAlertProps {
  workSecs: number;
  onDismiss: () => void;
}

function formatDuration(secs: number): string {
  const total = Math.floor(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}小时${m}分${s}秒`;
  if (m > 0) return `${m}分${s}秒`;
  return `${s}秒`;
}

const TIPS = [
  "站起来走动几分钟，促进血液循环",
  "做颈部旋转运动：缓慢左右转头各5次",
  "做肩部放松：耸肩后缓慢放下，重复10次",
  "远眺窗外20秒，缓解眼部疲劳",
  "做腰部伸展：双手上举，身体缓慢侧弯",
  "深呼吸5次，放松身心",
];

export default function BreakAlert({ workSecs, onDismiss }: BreakAlertProps) {
  const tip = TIPS[Math.floor(Math.random() * TIPS.length)];

  return (
    <div className="break-alert-overlay" onClick={onDismiss}>
      <div className="break-alert" onClick={(e) => e.stopPropagation()}>
        <div className="break-alert-icon">🧘</div>
        <h2>该休息了！</h2>
        <p>你已经连续工作了</p>
        <div className="work-time">{formatDuration(workSecs)}</div>
        <p>长时间久坐会损伤腰部和颈椎，请起来活动一下</p>
        <div className="tips">
          <strong>💡 健康提示：</strong>
          <br />
          {tip}
        </div>
        <div className="actions">
          <button className="btn btn-primary" onClick={onDismiss}>
            知道了，稍后休息
          </button>
        </div>
      </div>
    </div>
  );
}
