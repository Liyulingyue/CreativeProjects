import { Link } from 'react-router-dom'

export default function Home() {
  return (
    <div className="hero-section">
      <div className="hero-badge">
        <span>✨</span>
        英语学习助手
      </div>
      
      <h1 className="hero-title">
        轻松掌握
        <br />
        <span className="hero-title-accent">雅思词汇</span>
      </h1>
      
      <p className="hero-subtitle">
        收录 3000+ 雅思核心词汇，支持浏览、搜索、随机抽取等多种学习方式
      </p>
      
      <div className="hero-actions">
        <Link to="/vocab" className="hero-btn hero-btn-primary">
          <span>📖</span>
          开始学习
        </Link>
        <Link to="/random" className="hero-btn hero-btn-secondary">
          <span>🎲</span>
          随机抽取
        </Link>
      </div>
       
      <div className="hero-stats">
        <div className="hero-stat">
          <div className="hero-stat-value">3000+</div>
          <div className="hero-stat-label">词汇总量</div>
        </div>
        <div className="hero-stat">
          <div className="hero-stat-value">IELTS</div>
          <div className="hero-stat-label">考试类型</div>
        </div>
        <div className="hero-stat">
          <div className="hero-stat-value">100%</div>
          <div className="hero-stat-label">免费使用</div>
        </div>
      </div>
    </div>
  )
}
