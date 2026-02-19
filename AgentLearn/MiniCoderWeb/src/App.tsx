import React, { useState } from 'react';
import { CodeEditor } from './components/CodeEditor';
import { MiniCoderApi } from './services/api';
import { ApiResponse } from './types';
import './App.css';

type FunctionType = 'generate' | 'explain' | 'fix' | 'optimize';

function App() {
  const [activeFunction, setActiveFunction] = useState<FunctionType>('generate');
  const [inputCode, setInputCode] = useState('');
  const [outputCode, setOutputCode] = useState('');
  const [prompt, setPrompt] = useState('');
  const [language, setLanguage] = useState('python');
  const [errorMessage, setErrorMessage] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleGenerate = async () => {
    if (!prompt.trim()) {
      setError('请输入代码描述');
      return;
    }

    setLoading(true);
    setError(null);
    
    const result: ApiResponse = await MiniCoderApi.generateCode(prompt, language);
    
    if (result.success && result.data) {
      setOutputCode(result.data.content || '');
    } else {
      setError(result.error || '生成代码失败');
    }
    
    setLoading(false);
  };

  const handleExplain = async () => {
    if (!inputCode.trim()) {
      setError('请输入要解释的代码');
      return;
    }

    setLoading(true);
    setError(null);
    
    const result: ApiResponse = await MiniCoderApi.explainCode(inputCode);
    
    if (result.success && result.data) {
      setOutputCode(result.data.content || '');
    } else {
      setError(result.error || '解释代码失败');
    }
    
    setLoading(false);
  };

  const handleFix = async () => {
    if (!errorMessage.trim() || !inputCode.trim()) {
      setError('请输入错误信息和代码上下文');
      return;
    }

    setLoading(true);
    setError(null);
    
    const result: ApiResponse = await MiniCoderApi.fixBug(errorMessage, inputCode);
    
    if (result.success && result.data) {
      setOutputCode(result.data.content || '');
    } else {
      setError(result.error || '修复bug失败');
    }
    
    setLoading(false);
  };

  const handleOptimize = async () => {
    if (!inputCode.trim()) {
      setError('请输入要优化的代码');
      return;
    }

    setLoading(true);
    setError(null);
    
    const result: ApiResponse = await MiniCoderApi.optimizeCode(inputCode);
    
    if (result.success && result.data) {
      setOutputCode(result.data.content || '');
    } else {
      setError(result.error || '优化代码失败');
    }
    
    setLoading(false);
  };

  const handleSubmit = () => {
    switch (activeFunction) {
      case 'generate':
        handleGenerate();
        break;
      case 'explain':
        handleExplain();
        break;
      case 'fix':
        handleFix();
        break;
      case 'optimize':
        handleOptimize();
        break;
    }
  };

  return (
    <div className="app">
      <header className="app-header">
        <h1>🚀 MiniCoder Web</h1>
        <p>智能代码助手 - React + TypeScript</p>
      </header>

      <div className="app-container">
        {/* 功能选择 */}
        <div className="function-selector">
          <button
            className={activeFunction === 'generate' ? 'active' : ''}
            onClick={() => setActiveFunction('generate')}
          >
            ✨ 生成代码
          </button>
          <button
            className={activeFunction === 'explain' ? 'active' : ''}
            onClick={() => setActiveFunction('explain')}
          >
            📚 解释代码
          </button>
          <button
            className={activeFunction === 'fix' ? 'active' : ''}
            onClick={() => setActiveFunction('fix')}
          >
            🔧 修复bug
          </button>
          <button
            className={activeFunction === 'optimize' ? 'active' : ''}
            onClick={() => setActiveFunction('optimize')}
          >
            ⚡ 优化代码
          </button>
        </div>

        {/* 输入区域 */}
        <div className="input-section">
          <h3>输入</h3>
          {activeFunction === 'generate' ? (
            <div className="generate-inputs">
              <input
                type="text"
                placeholder="描述要生成的代码..."
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                className="prompt-input"
              />
              <select
                value={language}
                onChange={(e) => setLanguage(e.target.value)}
                className="language-select"
              >
                <option value="python">Python</option>
                <option value="javascript">JavaScript</option>
                <option value="typescript">TypeScript</option>
                <option value="java">Java</option>
                <option value="cpp">C++</option>
                <option value="go">Go</option>
              </select>
            </div>
          ) : (activeFunction === 'fix' ? (
            <div className="fix-inputs">
              <input
                type="text"
                placeholder="错误信息..."
                value={errorMessage}
                onChange={(e) => setErrorMessage(e.target.value)}
                className="error-input"
              />
              <CodeEditor
                code={inputCode}
                onCodeChange={setInputCode}
                placeholder="代码上下文..."
              />
            </div>
          ) : (
            <CodeEditor
              code={inputCode}
              onCodeChange={setInputCode}
              placeholder="请输入代码..."
            />
          )}
        </div>

        {/* 操作按钮 */}
        <div className="action-section">
          <button
            className="submit-button"
            onClick={handleSubmit}
            disabled={loading}
          >
            {loading ? '处理中...' : '执行'}
          </button>
          {error && <div className="error-message">❌ {error}</div>}
        </div>

        {/* 输出区域 */}
        <div className="output-section">
          <h3>输出</h3>
          <CodeEditor
            code={outputCode}
            onCodeChange={setOutputCode}
            readOnly={true}
          />
        </div>
      </div>
    </div>
  );
}

export default App;