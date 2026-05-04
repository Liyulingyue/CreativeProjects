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
  const [error, setError] = useState('')
  const inputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    window.onRecordingStarted = () => setState('recording')
    window.onRecordingStopped = () => setState('processing')
    window.onAsrResult = (t) => {
      setText(t)
      setState('idle')
      setError('')
    }
    window.onAsrError = (e) => {
      setError(e)
      setState('idle')
    }
    window.onPasteDone = () => {
      setText('')
      setError('')
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

  const pasteToActive = useCallback(() => {
    if (text) {
      sendIpc({ type: 'paste_text', value: text })
    }
  }, [text])

  const cancel = useCallback(() => {
    setText('')
    setError('')
  }, [])

  const handleDrag = useCallback((e: React.MouseEvent) => {
    if (e.button === 0) {
      sendIpc({ type: 'start_drag' })
    }
  }, [])

  return (
    <div className="app main">
      <div className="main-row">
        <div className="drag-handle" onMouseDown={handleDrag} title="Drag to move">⋮⋮</div>
        <textarea
          ref={inputRef}
          placeholder={error ? `Error: ${error}` : "Result..."}
          rows={2}
          className={`main-input ${error ? 'error' : ''}`}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <button
          className={`mic-btn ${state}`}
          onClick={toggleRecording}
          disabled={state === 'processing'}
          title={state === 'idle' ? 'Start recording' : state === 'recording' ? 'Stop recording' : 'Processing...'}
        >
          {state === 'idle' && (
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/>
              <path d="M19 10v2a7 7 0 0 1-14 0v-2"/>
              <line x1="12" y1="19" x2="12" y2="23"/>
              <line x1="8" y1="23" x2="16" y2="23"/>
            </svg>
          )}
          {state === 'recording' && (
            <svg viewBox="0 0 24 24" fill="currentColor">
              <rect x="6" y="6" width="12" height="12" rx="2"/>
            </svg>
          )}
          {state === 'processing' && (
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="spin">
              <circle cx="12" cy="12" r="10"/>
            </svg>
          )}
        </button>
        <button 
          className="btn success small" 
          onClick={pasteToActive} 
          disabled={!text}
          title="Paste to active window"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" style={{width:18,height:18}}>
            <polyline points="20 6 9 17 4 12"/>
          </svg>
        </button>
        <button className="btn ghost small" onClick={cancel} disabled={!text} title="Clear">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" style={{width:18,height:18}}>
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>
    </div>
  )
}

function SettingsWindow() {
  const [config, setConfig] = useState({
    hotkey: 'F13',
    asr_model: 'base',
    language: 'auto',
    always_on_top: true,
    server_port: 18789,
    audio_device: '',
  })
  const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([])
  const [audioLevel, setAudioLevel] = useState(0)
  const [isMonitoring, setIsMonitoring] = useState(false)

  useEffect(() => {
    window.onAudioDevices = (devices) => {
      console.log('[Settings] Received audio devices:', devices)
      setAudioDevices(devices)
    }
    window.onAudioLevel = (level) => {
      setAudioLevel(level)
    }
    
    sendIpc({ type: 'get_audio_devices' })
    
    return () => {
      if (isMonitoring) {
        sendIpc({ type: 'stop_audio_monitoring' })
      }
    }
  }, [])

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

  const saveConfig = () => {
    sendIpc({ type: 'save_config', value: JSON.stringify(config) })
  }

  return (
    <div className="app settings-window">
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
          <select value={config.hotkey} onChange={e => update('hotkey', e.target.value)}>
            <option value="F13">F13</option>
            <option value="F14">F14</option>
            <option value="F15">F15</option>
            <option value="F16">F16</option>
            <option value="F17">F17</option>
            <option value="F18">F18</option>
            <option value="F19">F19</option>
            <option value="F20">F20</option>
            <option value="CTRL+SPACE">Ctrl+Space</option>
          </select>
        </div>
        <div className="form-group">
          <label>ASR Model</label>
          <select value={config.asr_model} onChange={e => update('asr_model', e.target.value)}>
            <option value="tiny">tiny - fastest, lowest accuracy</option>
            <option value="base">base - recommended</option>
            <option value="small">small - higher accuracy</option>
            <option value="medium">medium - high accuracy (slow)</option>
          </select>
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
          <label>Server Port</label>
          <input
            type="number"
            value={config.server_port}
            onChange={e => update('server_port', parseInt(e.target.value) || 18789)}
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
        <p className="hint">
          WhisperServer must be started separately:<br/>
          <code>python server/whisper_server.py --model {config.asr_model}</code>
        </p>
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
