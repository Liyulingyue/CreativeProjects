import React, { useState } from 'react';
import './App.css';

interface ProxyResponse {
  status_code: number;
  data: any;
  headers: any;
  error: string | null;
}

function App() {
  const [url, setUrl] = useState('');
  const [method, setMethod] = useState('GET');
  const [response, setResponse] = useState<ProxyResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handleProxyRequest = async () => {
    if (!url) {
      setError('Please enter a URL');
      return;
    }

    setLoading(true);
    setError('');
    setResponse(null);

    try {
      const res = await fetch('http://localhost:8000/proxy', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ url, method }),
      });

      const data = await res.json();
      setResponse(data);
      
      if (data.error) {
        setError(data.error);
      }
    } catch (err: any) {
      setError(err.message || 'Failed to make proxy request');
    } finally {
      setLoading(false);
    }
  };

  const handleSimpleProxy = async () => {
    if (!url) {
      setError('Please enter a URL');
      return;
    }

    setLoading(true);
    setError('');
    setResponse(null);

    try {
      // 使用简单代理端点
      const proxyUrl = `http://localhost:8000/proxy/simple?url=${encodeURIComponent(url)}&method=${method}`;
      const res = await fetch(proxyUrl);
      const text = await res.text();
      
      setResponse({
        status_code: res.status,
        data: text,
        headers: {},
        error: null
      });
    } catch (err: any) {
      setError(err.message || 'Failed to make proxy request');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="App">
      <header className="App-header">
        <h1>MiniClaw Proxy Service</h1>
        <p>Lightweight proxy for private deployment</p>
      </header>

      <div className="proxy-container">
        <div className="input-group">
          <label>Target URL:</label>
          <input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://jsonplaceholder.typicode.com/posts/1"
          />
        </div>

        <div className="input-group">
          <label>Method:</label>
          <select value={method} onChange={(e) => setMethod(e.target.value)}>
            <option value="GET">GET</option>
            <option value="POST">POST</option>
            <option value="PUT">PUT</option>
            <option value="DELETE">DELETE</option>
          </select>
        </div>

        <div className="button-group">
          <button onClick={handleProxyRequest} disabled={loading}>
            {loading ? 'Loading...' : 'Send Proxy Request (POST)'}
          </button>
          <button onClick={handleSimpleProxy} disabled={loading}>
            {loading ? 'Loading...' : 'Simple Proxy (GET)'}
          </button>
        </div>

        {error && <div className="error">{error}</div>}

        {response && (
          <div className="response">
            <h3>Response (Status: {response.status_code}):</h3>
            <pre>{JSON.stringify(response.data, null, 2)}</pre>
            {response.headers && Object.keys(response.headers).length > 0 && (
              <div className="headers">
                <h4>Headers:</h4>
                <pre>{JSON.stringify(response.headers, null, 2)}</pre>
              </div>
            )}
          </div>
        )}

        <div className="test-urls">
          <h4>Test URLs:</h4>
          <ul>
            <li onClick={() => setUrl('https://jsonplaceholder.typicode.com/posts/1')}>
              https://jsonplaceholder.typicode.com/posts/1
            </li>
            <li onClick={() => setUrl('https://jsonplaceholder.typicode.com/users/1')}>
              https://jsonplaceholder.typicode.com/users/1
            </li>
            <li onClick={() => setUrl('https://httpbin.org/get')}>
              https://httpbin.org/get
            </li>
          </ul>
        </div>
      </div>

      <footer className="App-footer">
        <p>MiniClaw v1.0.0 | FastAPI + React + TypeScript | Proxy Enabled</p>
      </footer>
    </div>
  );
}

export default App;