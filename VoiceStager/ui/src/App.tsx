import { useState, useEffect, useCallback, useRef } from 'react'
import { sendIpc, setIpcBlock } from './ipc'
import { I18N, type Language } from './i18n'
import './App.css'

type RecordingState = 'idle' | 'recording' | 'processing'
type Page = 'main' | 'settings'

interface AudioDevice {
  id: string
  name: string
}

declare global {
  interface Window {
    onRecordingStarted: () => void
    onRecordingStopped: () => void
    onAsrResult: (text: string) => void
    onAsrError: (error: string) => void
    onPasteDone: () => void
    onClearText: () => void
    onAudioDevices: (devices: AudioDevice[]) => void
    onAudioLevel: (level: number) => void
    onConfigSync: (config: AppConfig) => void
  }
}

function getWindowType(): Page {
  const params = new URLSearchParams(window.location.search)
  return (params.get('window') as Page) || 'main'
}

function detectBrowserLanguage(): Language {
  return navigator.language.toLowerCase().startsWith('zh') ? 'zh' : 'en'
}

function useI18n() {
  const [lang, setLang] = useState<Language>(() => {
    const saved = localStorage.getItem('vstage_lang')
    return (saved as Language) || detectBrowserLanguage()
  })

  useEffect(() => {
    localStorage.setItem('vstage_lang', lang)
  }, [lang])

  return { lang, setLang, t: I18N[lang] }
}

function MainWindow() {
  const [state, setState] = useState<RecordingState>('idle')
  const [text, setText] = useState('')
  const [audioLevel, setAudioLevel] = useState(0)
  const inputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    window.onRecordingStarted = () => setState('recording')
    window.onRecordingStopped = () => { setState('processing'); setAudioLevel(0) }
    window.onAsrResult = (t) => {
      setText(t)
      sendIpc({ type: 'update_current_text', value: t })
      setState('idle')
    }
    window.onAsrError = () => setState('idle')
    window.onPasteDone = () => {
      setText('')
      sendIpc({ type: 'clear_current_text' })
    }
    window.onClearText = () => setText('')
    window.onAudioLevel = (level) => setAudioLevel(level)
  }, [])

  const toggleRecording = useCallback(() => {
    if (state === 'idle') {
      setState('recording')
      sendIpc({ type: 'start_recording' })
    } else if (state === 'recording') {
      setState('processing')
      sendIpc({ type: 'stop_recording' })
    }
  }, [state])

  const confirmText = useCallback(() => {
    if (text.trim()) sendIpc({ type: 'paste_text', value: text })
  }, [text])

  const handleDragMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button === 0) sendIpc({ type: 'start_drag' })
  }, [])

  const handleDragContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    sendIpc({
      type: 'show_native_menu',
      value: JSON.stringify({
        x: e.clientX,
        y: e.clientY,
        text,
        hasText: text.trim().length > 0,
        isRecording: state === 'recording',
        isProcessing: state === 'processing',
      })
    })
  }, [state, text])

  const hasText = text.trim().length > 0

  return (
    <div className="app main">
      <div className="main-row">
        <textarea
          ref={inputRef}
          placeholder={state === 'recording' ? '' : state === 'processing' ? 'Recognizing...' : 'Result...'}
          rows={2}
          className={`main-input${state === 'recording' ? ' recording' : ''}`}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        {hasText ? (
          <button className="confirm-btn" onClick={confirmText} title="Confirm">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
          </button>
        ) : (
          <button
            className={`mic-btn ${state}`}
            onClick={toggleRecording}
            disabled={state === 'processing'}
            style={state === 'recording' ? {
              boxShadow: `0 0 ${6 + audioLevel * 24}px ${2 + audioLevel * 14}px rgba(239,68,68,${0.35 + audioLevel * 0.5})`,
              transform: `scale(${1 + audioLevel * 0.12})`,
            } : undefined}
            title={state === 'idle' ? 'Start recording' : state === 'recording' ? 'Stop recording' : 'Processing...'}
          >
            {(state === 'idle' || state === 'recording') && (
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/>
                <path d="M19 10v2a7 7 0 0 1-14 0v-2"/>
                <line x1="12" y1="19" x2="12" y2="23"/>
                <line x1="8" y1="23" x2="16" y2="23"/>
              </svg>
            )}
            {state === 'processing' && (
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="spin">
                <circle cx="12" cy="12" r="10"/>
              </svg>
            )}
          </button>
        )}
        <div
          className="drag-handle"
          onMouseDown={handleDragMouseDown}
          onContextMenu={handleDragContextMenu}
          title="Left: Drag, Right: Menu"
        >
          <svg viewBox="0 0 24 24" fill="currentColor" style={{width:12,height:12}}>
            <circle cx="8" cy="6" r="1.5"/>
            <circle cx="16" cy="6" r="1.5"/>
            <circle cx="8" cy="12" r="1.5"/>
            <circle cx="16" cy="12" r="1.5"/>
            <circle cx="8" cy="18" r="1.5"/>
            <circle cx="16" cy="18" r="1.5"/>
          </svg>
        </div>
      </div>
    </div>
  )
}

function SettingsWindow() {
  const { lang, setLang, t } = useI18n()
  const [config, setConfig] = useState({
    record_hotkey: 'F13',
    send_hotkey: 'F14',
    language: 'auto',
    always_on_top: true,
    server_url: 'http://127.0.0.1:18789',
    audio_device: '',
    asr_mode: 'local',
    local_model: 'sensevoice-small',
    use_buffer: true,
  })
  const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([])
  const [audioLevel, setAudioLevel] = useState(0)
  const [isMonitoring, setIsMonitoring] = useState(false)
  const [recordHotkeyInput, setRecordHotkeyInput] = useState('')
  const [sendHotkeyInput, setSendHotkeyInput] = useState('')
  const [isRecordingRecordHotkey, setIsRecordingRecordHotkey] = useState(false)
  const [isRecordingSendHotkey, setIsRecordingSendHotkey] = useState(false)
  const [showSavedMsg, setShowSavedMsg] = useState(false)
  const [showModelHelp, setShowModelHelp] = useState(false)

  useEffect(() => {
    window.onAudioDevices = (devices) => setAudioDevices(devices)
    window.onAudioLevel = (level) => setAudioLevel(level)
    window.onConfigSync = (newConfig) => {
      setConfig(newConfig)
      setRecordHotkeyInput(newConfig.record_hotkey)
      setSendHotkeyInput(newConfig.send_hotkey)
    }
    sendIpc({ type: 'get_audio_devices' })
    sendIpc({ type: 'get_config' })
    return () => {
      if (isMonitoring) sendIpc({ type: 'stop_audio_monitoring' })
    }
  }, [])

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!isRecordingRecordHotkey && !isRecordingSendHotkey) return
      if (e.key === 'Control' || e.key === 'Alt' || e.key === 'Shift' || e.key === 'Meta') return
      e.preventDefault()
      e.stopPropagation()
      const parts: string[] = []
      if (e.ctrlKey) parts.push('Ctrl')
      if (e.altKey) parts.push('Alt')
      if (e.shiftKey) parts.push('Shift')
      
      let keyDisplay = e.key.toUpperCase()
      if (e.code === 'Space') keyDisplay = 'SPACE'
      else if (e.code.startsWith('Key')) keyDisplay = e.code.slice(3)
      else if (e.code.startsWith('Digit')) keyDisplay = e.code.slice(5)
      
      if (!['CONTROL', 'ALT', 'SHIFT', 'META'].includes(keyDisplay)) {
        parts.push(keyDisplay)
      }

      if (parts.length > 0) {
        const hotkey = parts.join('+')
        if (isRecordingRecordHotkey) {
          setRecordHotkeyInput(hotkey)
          setConfig(prev => ({ ...prev, record_hotkey: hotkey }))
          setIsRecordingRecordHotkey(false)
          setIpcBlock(false)
        } else {
          setSendHotkeyInput(hotkey)
          setConfig(prev => ({ ...prev, send_hotkey: hotkey }))
          setIsRecordingSendHotkey(false)
          setIpcBlock(false)
        }
      }
    }
    if (isRecordingRecordHotkey || isRecordingSendHotkey) {
      window.addEventListener('keydown', handleKeyDown, true)
    }
    return () => window.removeEventListener('keydown', handleKeyDown, true)
  }, [isRecordingRecordHotkey, isRecordingSendHotkey])

  const update = (key: string, value: string | number | boolean) => {
    setConfig(prev => ({ ...prev, [key]: value }))
  }

  const handleAudioDeviceChange = (deviceId: string) => {
    update('audio_device', deviceId)
    sendIpc({ type: 'select_audio_device', value: deviceId })
  }

  /*
  useEffect(() => {
    // 监听 config 变化，自动保存
    if (Object.keys(config).length > 0) {
      sendIpc({ type: 'save_config', value: JSON.stringify(config) })
    }
  }, [config])
  */

  const toggleMonitoring = () => {
    if (isMonitoring) {
      sendIpc({ type: 'stop_audio_monitoring' })
      setIsMonitoring(false)
    } else {
      sendIpc({ type: 'start_audio_monitoring' })
      setIsMonitoring(true)
    }
  }

  const toggleRecordHotkeyRecording = () => {
    if (isRecordingRecordHotkey) {
      setIsRecordingRecordHotkey(false)
      setIpcBlock(false)
    } else {
      setRecordHotkeyInput('')
      setIsRecordingRecordHotkey(true)
      setIsRecordingSendHotkey(false)
      setIpcBlock(true)
    }
  }

  const toggleSendHotkeyRecording = () => {
    if (isRecordingSendHotkey) {
      setIsRecordingSendHotkey(false)
      setIpcBlock(false)
    } else {
      setSendHotkeyInput('')
      setIsRecordingSendHotkey(true)
      setIsRecordingRecordHotkey(false)
      setIpcBlock(true)
    }
  }

  const saveConfig = () => {
    // 显式点击保存依然保留，用于显示吐司提示，但实际上 useEffect 已经实时保存了
    sendIpc({ type: 'save_config', value: JSON.stringify(config) })
    setIsRecordingRecordHotkey(false)
    setIsRecordingSendHotkey(false)
    setShowSavedMsg(true)
    setTimeout(() => setShowSavedMsg(false), 2000)
  }

  const handleToggle = (key: keyof AppConfig) => {
    setConfig(prev => ({ ...prev, [key]: !prev[key] }))
  }

  const handleInput = (key: keyof AppConfig, val: string) => {
    setConfig(prev => ({ ...prev, [key]: val }))
  }

  return (
    <div className="app settings-window">
      {showSavedMsg && (
        <div className="toast-msg">{t.saved}</div>
      )}
      {showModelHelp && (
        <div className="modal-overlay" onClick={() => setShowModelHelp(false)}>
          <div className="modal-content" onClick={e => e.stopPropagation()}>
            <h3>{t.helpTitle}</h3>
            <p>{t.helpDesc}</p>
            <p>{t.helpStep1}</p>
            <p>{t.helpStep2}<br />
              <code>VoiceStager/models/sensevoice-small/</code>
            </p>
            <p>{t.helpSource}: <a href="https://www.modelscope.cn/models/pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/files" target="_blank" style={{color: '#3b82f6'}}>ModelScope</a></p>
            <button className="btn primary modal-close-btn" onClick={() => setShowModelHelp(false)}>{t.helpBtn}</button>
          </div>
        </div>
      )}
      <div className="settings-header">
        <span className="settings-title">{t.title}</span>
        <button
          className="lang-toggle"
          onClick={() => setLang(lang === 'zh' ? 'en' : 'zh')}
          title="Switch Language"
        >
          {lang === 'zh' ? 'EN' : '中'}
        </button>
      </div>
      <div className="settings-body">
        <div className="form-group">
          <label>{t.microphone}</label>
          <div className="audio-device-row">
            <select
              value={config.audio_device}
              onChange={e => handleAudioDeviceChange(e.target.value)}
            >
              <option value="">{t.defaultMic}</option>
              {audioDevices
                .filter(d => d.name && d.name.trim() !== '')
                .map(d => (
                  <option key={d.id} value={d.id}>{d.name}</option>
                ))}
            </select>
            <button
              className={`btn ${isMonitoring ? 'danger' : 'secondary'} small`}
              onClick={toggleMonitoring}
            >
              {isMonitoring ? t.stop : t.test}
            </button>
          </div>
          {isMonitoring && (
            <div className="audio-level-bar">
              <div className="audio-level-fill" style={{ width: `${audioLevel * 100}%` }} />
            </div>
          )}
        </div>
        <div className="form-group">
          <label>{t.recordHotkey}</label>
          <div className="audio-device-row">
            <input
              type="text"
              value={recordHotkeyInput || config.record_hotkey}
              readOnly
              className="hotkey-input"
              placeholder={isRecordingRecordHotkey ? t.pressHotkey : ''}
            />
            <button
              className={`btn ${isRecordingRecordHotkey ? 'primary' : 'secondary'} small`}
              onClick={toggleRecordHotkeyRecording}
            >
              {isRecordingRecordHotkey ? t.recording : t.record}
            </button>
          </div>
        </div>
        <div className="form-group">
          <label>{t.sendHotkey}</label>
          <div className="audio-device-row">
            <input
              type="text"
              value={sendHotkeyInput || config.send_hotkey}
              readOnly
              className="hotkey-input"
              placeholder={isRecordingSendHotkey ? t.pressHotkey : ''}
            />
            <button
              className={`btn ${isRecordingSendHotkey ? 'primary' : 'secondary'} small`}
              onClick={toggleSendHotkeyRecording}
            >
              {isRecordingSendHotkey ? t.recording : t.record}
            </button>
          </div>
        </div>
        <div className="form-group">
          <label>{t.language}</label>
          <select value={config.language} onChange={e => update('language', e.target.value)}>
            <option value="auto">{t.auto}</option>
            <option value="zh">中文</option>
            <option value="en">English</option>
            <option value="ja">日本語</option>
            <option value="ko">한국어</option>
          </select>
        </div>
        <div className="form-group">
          <label>{t.asrMode}</label>
          <select value={config.asr_mode} onChange={e => update('asr_mode', e.target.value)}>
            <option value="local">{t.localMode}</option>
            <option value="remote">{t.remoteMode}</option>
          </select>
        </div>
        <div className="form-group" style={{ opacity: config.asr_mode === 'local' ? 1 : 0.5 }}>
          <div className="label-with-help">
            <label>{t.localModel}</label>
            <span className="help-icon" onClick={() => setShowModelHelp(true)} title="Help">
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="12" cy="12" r="10"/>
                <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/>
                <line x1="12" y1="17" x2="12.01" y2="17"/>
              </svg>
            </span>
          </div>
          <select
            value={config.local_model}
            disabled={config.asr_mode !== 'local'}
            onChange={e => update('local_model', e.target.value)}
          >
            <option value="sensevoice-small">{t.float32}</option>
            <option value="sensevoice-small-int8">{t.int8}</option>
          </select>
        </div>
        <div className="form-group" style={{ opacity: config.asr_mode === 'remote' ? 1 : 0.5 }}>
          <label>{t.serverUrl}</label>
          <input
            type="text"
            value={config.server_url}
            disabled={config.asr_mode !== 'remote'}
            onChange={e => update('server_url', e.target.value)}
          />
        </div>
        <div className="form-group checkbox-group">
          <label title={t.useBufferHint}>
            <input
              type="checkbox"
              checked={(config as any).use_buffer}
              onChange={e => update('use_buffer', e.target.checked)}
            />
            {t.useBuffer}
          </label>
        </div>
        <div className="form-group checkbox-group">
          <label>
            <input
              type="checkbox"
              checked={(config as any).always_on_top}
              onChange={e => update('always_on_top', e.target.checked)}
            />
            {t.alwaysOnTop}
          </label>
        </div>
        <button className="btn primary" onClick={saveConfig}>{t.save}</button>
        <div className="hint-container">
          <p className="hint">
            <strong>Local:</strong> {t.localHint}
          </p>
          <p className="hint">
            <strong>Remote:</strong> <code>{t.remoteHint}</code>
          </p>
        </div>
      </div>
    </div>
  )
}

function App() {
  const page = getWindowType()
  if (page === 'settings') {
    return <SettingsWindow />
  }
  return <MainWindow />
}

export default App
