import { useState, useEffect, useCallback, useRef } from 'react'
import { sendIpc } from './ipc'
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
  }
}

function getWindowType(): Page {
  const params = new URLSearchParams(window.location.search)
  return (params.get('window') as Page) || 'main'
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
      setState('idle')
    }
    window.onAsrError = () => {
      setState('idle')
    }
    window.onPasteDone = () => {
      setText('')
    }
    window.onClearText = () => {
      setText('')
    }
    window.onAudioLevel = (level) => {
      setAudioLevel(level)
    }
  }, [])

  const toggleRecording = useCallback(() => {
    console.log('[Frontend] toggleRecording called, state:', state);
    if (state === 'idle') {
      setState('recording')
      console.log('[Frontend] Sending start_recording');
      sendIpc({ type: 'start_recording' })
    } else if (state === 'recording') {
      setState('processing')
      console.log('[Frontend] Sending stop_recording');
      sendIpc({ type: 'stop_recording' })
    }
  }, [state])

  const confirmText = useCallback(() => {
    if (text.trim()) {
      sendIpc({ type: 'paste_text', value: text })
    }
  }, [text])

  const handleDragMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button === 0) {
      sendIpc({ type: 'start_drag' })
    }
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
          <button
            className="confirm-btn"
            onClick={confirmText}
            title="Confirm"
          >
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
  const [config, setConfig] = useState({
    hotkey: 'F13',
    language: 'auto',
    always_on_top: true,
    server_url: 'http://127.0.0.1:18789',
    audio_device: '',
    asr_mode: 'local',
    local_model: 'sensevoice-small',
  })
  const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([])
  const [audioLevel, setAudioLevel] = useState(0)
  const [isMonitoring, setIsMonitoring] = useState(false)
  const [hotkeyInput, setHotkeyInput] = useState('')
  const [isRecordingHotkey, setIsRecordingHotkey] = useState(false)
  const [showSavedMsg, setShowSavedMsg] = useState(false)
  const [showModelHelp, setShowModelHelp] = useState(false)

  useEffect(() => {
    window.onAudioDevices = (devices) => {
      console.log('[Settings] Received audio devices:', devices)
      setAudioDevices(devices)
    }
    window.onAudioLevel = (level) => {
      setAudioLevel(level)
    }
    
    sendIpc({ type: 'get_audio_devices' })
    setHotkeyInput(config.hotkey)
    
    return () => {
      if (isMonitoring) {
        sendIpc({ type: 'stop_audio_monitoring' })
      }
    }
  }, [])

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!isRecordingHotkey) return
      e.preventDefault()
      e.stopPropagation()
      
      const parts: string[] = []
      if (e.ctrlKey) parts.push('Ctrl')
      if (e.altKey) parts.push('Alt')
      if (e.shiftKey) parts.push('Shift')
      if (e.key === ' ') parts.push('Space')
      else if (e.key.startsWith('F') && e.key.length <= 3) parts.push(e.key.toUpperCase())
      else if (e.key.length === 1) parts.push(e.key.toUpperCase())
      
      if (parts.length > 0 && (parts.length > 1 || parts[0].startsWith('F'))) {
        const hotkey = parts.join('+')
        setHotkeyInput(hotkey)
        setConfig(prev => ({ ...prev, hotkey }))
        setIsRecordingHotkey(false)
      }
    }

    if (isRecordingHotkey) {
      window.addEventListener('keydown', handleKeyDown, true)
    }
    return () => {
      window.removeEventListener('keydown', handleKeyDown, true)
    }
  }, [isRecordingHotkey])

  const update = (key: string, value: string | number | boolean) => {
    setConfig(prev => ({ ...prev, [key]: value }))
  }

  const handleAudioDeviceChange = (deviceId: string) => {
    update('audio_device', deviceId)
    sendIpc({ type: 'select_audio_device', value: deviceId })
  }

  const toggleMonitoring = () => {
    if (isMonitoring) {
      sendIpc({ type: 'stop_audio_monitoring' })
      setIsMonitoring(false)
    } else {
      sendIpc({ type: 'start_audio_monitoring' })
      setIsMonitoring(true)
    }
  }

  const toggleHotkeyRecording = () => {
    if (isRecordingHotkey) {
      setIsRecordingHotkey(false)
    } else {
      setHotkeyInput('')
      setIsRecordingHotkey(true)
    }
  }

  const saveConfig = () => {
    sendIpc({ type: 'save_config', value: JSON.stringify(config) })
    setIsRecordingHotkey(false)
    setShowSavedMsg(true)
    setTimeout(() => setShowSavedMsg(false), 2000)
  }

  return (
    <div className="app settings-window">
      {showSavedMsg && (
        <div className="toast-msg">Settings Saved!</div>
      )}
      {showModelHelp && (
        <div className="modal-overlay" onClick={() => setShowModelHelp(false)}>
          <div className="modal-content" onClick={e => e.stopPropagation()}>
            <h3>Local Model Download Guide</h3>
            <p>If auto-download fails, please manually download files from ModelScope or HuggingFace:</p>
            <p>
              1. Download <code>model.int8.onnx</code> (or <code>model.onnx</code>) and <code>tokens.txt</code>.
            </p>
            <p>
              2. Place them in: <br/>
              <code>VoiceStager/models/sensevoice-small/</code>
            </p>
            <p>
              Source: <a href="https://www.modelscope.cn/models/pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/files" target="_blank" style={{color: '#3b82f6'}}>ModelScope Files</a>
            </p>
            <button className="btn primary modal-close-btn" onClick={() => setShowModelHelp(false)}>Got it</button>
          </div>
        </div>
      )}
      <div className="settings-header">
        <span className="settings-title">V-Stage Settings</span>
      </div>
      <div className="settings-body">
        <div className="form-group">
          <label>Microphone</label>
          <div className="audio-device-row">
            <select 
              value={config.audio_device} 
              onChange={e => handleAudioDeviceChange(e.target.value)}
            >
              <option value="">Default Microphone</option>
              {audioDevices
                .filter(d => d.name && d.name.trim() !== '')
                .map(d => (
                  <option key={d.id} value={d.id}>{d.name}</option>
                ))}
            </select>
            <button 
              className={`btn ${isMonitoring ? 'danger' : 'secondary'} small`}
              onClick={toggleMonitoring}
              title={isMonitoring ? 'Stop monitoring' : 'Test microphone'}
            >
              {isMonitoring ? 'Stop' : 'Test'}
            </button>
          </div>
          {isMonitoring && (
            <div className="audio-level-bar">
              <div className="audio-level-fill" style={{ width: `${audioLevel * 100}%` }} />
            </div>
          )}
        </div>
        <div className="form-group">
          <label>Hotkey</label>
          <div className="audio-device-row">
            <input
              type="text"
              value={hotkeyInput || config.hotkey}
              readOnly
              className="hotkey-input"
              placeholder={isRecordingHotkey ? 'Press keys...' : 'Click Record to set hotkey'}
            />
            <button 
              className={`btn ${isRecordingHotkey ? 'danger' : 'secondary'} small`}
              onClick={toggleHotkeyRecording}
              title={isRecordingHotkey ? 'Cancel' : 'Record hotkey'}
            >
              {isRecordingHotkey ? 'Cancel' : 'Record'}
            </button>
          </div>
          {isRecordingHotkey && (
            <p className="hint">Press any key combination (e.g., Ctrl+Space, F13)</p>
          )}
        </div>
        <div className="form-group">
          <label>Language</label>
          <select value={config.language} onChange={e => update('language', e.target.value)}>
            <option value="auto">Auto detect</option>
            <option value="zh">Chinese</option>
            <option value="en">English</option>
            <option value="ja">Japanese</option>
            <option value="ko">Korean</option>
          </select>
        </div>
        <div className="form-group">
          <label>ASR Mode</label>
          <select value={config.asr_mode} onChange={e => update('asr_mode', e.target.value)}>
            <option value="local">Local (Sherpa-Onnx)</option>
            <option value="remote">Remote (Server URL)</option>
          </select>
        </div>
        <div className="form-group" style={{ opacity: config.asr_mode === 'local' ? 1 : 0.5 }}>
          <div className="label-with-help">
            <label>Local Model</label>
            <span className="help-icon" onClick={() => setShowModelHelp(true)} title="Help: Manual download guide">
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="12" cy="12" r="10"></circle>
                <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"></path>
                <line x1="12" y1="17" x2="12.01" y2="17"></line>
              </svg>
            </span>
          </div>
          <select
            value={config.local_model}
            disabled={config.asr_mode !== 'local'}
            onChange={e => update('local_model', e.target.value)}
          >
            <option value="sensevoice-small">sensevoice-small (float32)</option>
            <option value="sensevoice-small-int8">sensevoice-small-int8 (int8, faster)</option>
          </select>
        </div>
        <div className="form-group" style={{ opacity: config.asr_mode === 'remote' ? 1 : 0.5 }}>
          <label>Server URL</label>
          <input
            type="text"
            value={config.server_url}
            disabled={config.asr_mode !== 'remote'}
            onChange={e => update('server_url', e.target.value)}
          />
        </div>
        <div className="form-group checkbox-group">
          <label>
            <input
              type="checkbox"
              checked={config.always_on_top}
              onChange={e => update('always_on_top', e.target.checked)}
            />
            Always on top
          </label>
        </div>
        <button className="btn primary" onClick={saveConfig}>Save</button>
        <div className="hint-container">
          <p className="hint">
            <strong>Local:</strong> Place models in <code>models/</code> (e.g. <code>models/sensevoice-small/model.onnx</code>).
          </p>
          <p className="hint">
            <strong>Remote:</strong> <code>python server/funasr_server.py --model sensevoice</code>
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
