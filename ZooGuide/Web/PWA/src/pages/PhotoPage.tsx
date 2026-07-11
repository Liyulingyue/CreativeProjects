import { useState } from 'react'
import { PhotoEvalDialog } from '../components/PhotoEvalDialog'

export function PhotoPage() {
  const [evalOpen, setEvalOpen] = useState(false)

  return (
    <div>
      <div className="card" style={{ background: 'linear-gradient(135deg, var(--primary-soft), #fff)' }}>
        <h3 className="card-title">📸 出片彩蛋</h3>
        <p style={{ fontSize: 13, color: 'var(--fg)', lineHeight: 1.6, margin: '8px 0 14px' }}>
          在红山拍一张动物照片，Agent 帮你打分 + 出徽章。识别出对应场馆时，会自动打卡。
        </p>
        <button className="btn btn-primary btn-full" onClick={() => setEvalOpen(true)}>
          📷 来一张
        </button>
      </div>

      <div className="card" style={{ marginTop: 14 }}>
        <h3 className="card-title">💡 拍摄小贴士</h3>
        <div style={{ fontSize: 13, color: 'var(--fg)', lineHeight: 1.8 }}>
          <div style={{ marginBottom: 6 }}>
            <strong>⏰ 最佳时机：</strong>上午 9-10 点（动物活跃）；下午 2-3 点（午睡醒来）
          </div>
          <div style={{ marginBottom: 6 }}>
            <strong>📷 角度：</strong>低角度仰拍动物，让动物"俯视"镜头
          </div>
          <div style={{ marginBottom: 6 }}>
            <strong>💡 光线：</strong>手机贴玻璃时关掉闪光灯，避免反光
          </div>
          <div>
            <strong>🎯 焦点：</strong>点击屏幕锁定动物眼睛对焦
          </div>
        </div>
      </div>

      <div className="card" style={{ marginTop: 14 }}>
        <h3 className="card-title">🏅 徽章一览</h3>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {[
            { emoji: '🐼', name: '国宝认证', v: 'panda' },
            { emoji: '🦍', name: '野菜F4认证', v: 'gorilla' },
            { emoji: '🐯', name: '百兽之王', v: 'tiger' },
            { emoji: '🦒', name: '长颈代表', v: 'giraffe' },
            { emoji: '🐨', name: '澳洲睡眠代言', v: 'koala' },
            { emoji: '🦊', name: '站岗小队长', v: 'meerkat' },
            { emoji: '🐾', name: '撞脸不撞DNA', v: 'red_panda' },
            { emoji: '🏔️', name: '首发游客', v: 'tangjiahe' },
          ].map((b) => (
            <div key={b.v} className="badge-row">
              <span style={{ fontSize: 20 }}>{b.emoji}</span>
              <span style={{ fontWeight: 600, color: 'var(--primary-strong)', fontSize: 13 }}>
                {b.name}
              </span>
              <span
                style={{
                  marginLeft: 'auto',
                  fontSize: 11,
                  color: 'var(--fg-muted)',
                }}
              >
                拍到对应动物解锁
              </span>
            </div>
          ))}
        </div>
      </div>

      {evalOpen && <PhotoEvalDialog onClose={() => setEvalOpen(false)} />}
    </div>
  )
}