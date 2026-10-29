import { Component, type ErrorInfo, type ReactNode } from 'react';

/**
 * Keeps a rendering error from taking the whole app with it.
 *
 * Without this, one bad render leaves a blank page and the only way
 * out is a refresh -- which loses whatever the user was in the middle
 * of. The error is shown instead, with a way back.
 */
interface Props {
  children: ReactNode;
  /** Shown instead of the default panel. */
  fallback?: (error: Error, reset: () => void) => ReactNode;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('渲染出错:', error, info.componentStack);
  }

  reset = () => this.setState({ error: null });

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    if (this.props.fallback) return this.props.fallback(error, this.reset);

    return (
      <div className="h-full overflow-y-auto bg-slate-50 flex items-center justify-center p-6">
        <div className="max-w-lg bg-white rounded-xl border border-slate-200 p-6 shadow-sm">
          <div className="text-base font-semibold text-slate-800 mb-2">这个页面出了点问题</div>
          <p className="text-sm text-slate-500 mb-4 leading-relaxed">
            页面在渲染的时候出错了。下面的信息可以帮助定位问题，其它页面不受影响。
          </p>
          <pre className="text-[11px] text-slate-500 bg-slate-50 rounded-lg p-3 overflow-auto max-h-40 whitespace-pre-wrap break-words mb-4">
            {error.message}
          </pre>
          <div className="flex gap-2">
            <button onClick={this.reset}
              className="px-3 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors">
              重试
            </button>
            <button onClick={() => { window.location.href = '/'; }}
              className="px-3 py-2 rounded-lg bg-slate-100 text-slate-600 text-sm font-medium hover:bg-slate-200 transition-colors">
              回到文件管理
            </button>
          </div>
        </div>
      </div>
    );
  }
}
