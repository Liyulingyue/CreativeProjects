interface Props {
  onStart: () => void
}

export function Home({ onStart }: Props) {
  return (
    <div>
      <div className="home-hero">
        <div className="emoji">🦒</div>
        <h2>逛红山，不必人挤人</h2>
        <p>
          告诉我你的时间、体力、带没带娃、怕不怕晒，
          我帮你定制一趟只属于你的红山路线。
        </p>
      </div>

      <ul className="feature-list">
        <li>
          <span className="icon">🧭</span>
          <div className="text">
            个性化路线
            <div className="desc">基于你的偏好，从 23 个场馆中精选</div>
          </div>
        </li>
        <li>
          <span className="icon">💬</span>
          <div className="text">
            叙事化讲解
            <div className="desc">同一只长臂猿，对年轻人 vs 带娃家长讲法不同</div>
          </div>
        </li>
        <li>
          <span className="icon">🔄</span>
          <div className="text">
            动态调整
            <div className="desc">走累了？太阳晒？一键重新规划后半段</div>
          </div>
        </li>
        <li>
          <span className="icon">🦒</span>
          <div className="text">
            动物打卡
            <div className="desc">逛完积累成就，记录你的红山之旅</div>
          </div>
        </li>
      </ul>

      <button className="btn btn-primary btn-full" onClick={onStart}>
        开始定制我的路线 ✨
      </button>

      <div className="footer-link">
        南京红山森林动物园 · 中国第一个取消动物表演的动物园
      </div>
    </div>
  )
}