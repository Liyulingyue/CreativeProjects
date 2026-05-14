import { OpenCodeCore, OpenCodeBackend } from '../core/OpenCodeCore';
import { OpenCodeApiClient, OpenCodeSession } from '../core/OpenCodeApiClient';
import http from '@ohos.net.http';

export interface FileNode {
  name: string;
  path: string;
  absolute: string;
  type: 'file' | 'directory';
  ignored: boolean;
}

export interface SessionDetailState {
  title: string;
  directory: string;
  backendId: string;
  backendUrl: string;
  authToken: string;
  isEditing: boolean;
  editingId: string;
  selectedRemoteSessionId: string;
  remoteSessions: OpenCodeSession[];
  isLoadingRemoteSessions: boolean;
  recentPaths: string[];
  pathSearchResults: FileNode[];
  isSearchingPath: boolean;
}

export class SessionDetailViewModel {
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  private currentRequest: http.HttpRequest | null = null;
  private pathSearchTimer: number = -1;

  fetchRemoteSessions(backendUrl: string, authToken: string, directory: string): Promise<OpenCodeSession[]> {
    return new Promise(async (resolve) => {
      if (!backendUrl) {
        resolve([]);
        return;
      }
      try {
        const client = new OpenCodeApiClient(backendUrl, authToken, directory);
        const sessions = await client.listSessions();
        console.info('[SessionDetailViewModel] Fetched remote sessions:', sessions.length);
        resolve(sessions);
      } catch (e) {
        console.error('[SessionDetailViewModel] Fetch remote sessions error:', e);
        resolve([]);
      }
    });
  }

  searchPaths(
    backendUrl: string,
    authToken: string,
    directory: string,
    query: string,
    fullPath: string | undefined,
    onResult: (results: FileNode[]) => void,
    onSearching: (isSearching: boolean) => void,
    currentRequest: http.HttpRequest | null,
    onRequestChange: (req: http.HttpRequest | null) => void
  ): void {
    if (!backendUrl || backendUrl === 'http://') {
      onSearching(false);
      return;
    }
    onSearching(true);
    this.cancelRequest(currentRequest, onRequestChange);

    const request = http.createHttp();
    onRequestChange(request);
    const url = backendUrl.replace(/\/+$/, '') + '/find/file';
    const params = `query=${encodeURIComponent(query)}&type=directory&limit=50`;

    request.request(
      url + '?' + params,
      {
        method: http.RequestMethod.GET,
        header: this.getHeaders(backendUrl, authToken, directory),
        connectTimeout: 5000,
        readTimeout: 5000,
      },
      (err, data) => {
        onSearching(false);
        onRequestChange(null);
        if (!err && data.responseCode === 200) {
          try {
            const parsed = JSON.parse(data.result as string) as string[];
            if (Array.isArray(parsed)) {
              const uniquePaths = Array.from(new Set(parsed));
              let results = uniquePaths.map((p: string): FileNode => {
                const absolute = p.startsWith('/') ? p : '/' + p;
                return {
                  name: absolute.split('/').pop() || absolute,
                  path: p,
                  absolute: absolute,
                  type: 'directory',
                  ignored: false
                };
              });

              if (fullPath) {
                const filterPath = fullPath.toLowerCase();
                results = results.filter(r => {
                  const rAbs = r.absolute.toLowerCase();
                  if (!rAbs.startsWith(filterPath)) return false;
                  if (rAbs === filterPath || rAbs === filterPath + '/') return false;
                  let remaining = rAbs.substring(filterPath.length);
                  if (remaining.startsWith('/')) {
                    remaining = remaining.substring(1);
                  }
                  const lastSlash = remaining.lastIndexOf('/');
                  if (lastSlash !== -1 && lastSlash !== remaining.length - 1) {
                    return false;
                  }
                  return true;
                });
              }

              results.sort((a, b) => a.absolute.localeCompare(b.absolute));
              onResult(results);
            }
          } catch (e) {
            console.error('[SessionDetailViewModel] Parse search error:', e);
            onResult([]);
          }
        } else {
          onResult([]);
        }
      }
    );
  }

  listDirectory(
    backendUrl: string,
    authToken: string,
    directory: string,
    path: string,
    onResult: (results: FileNode[]) => void,
    onSearching: (isSearching: boolean) => void,
    currentRequest: http.HttpRequest | null,
    onRequestChange: (req: http.HttpRequest | null) => void
  ): void {
    if (!backendUrl || backendUrl === 'http://') {
      onSearching(false);
      return;
    }
    onSearching(true);
    this.cancelRequest(currentRequest, onRequestChange);

    const request = http.createHttp();
    onRequestChange(request);
    const url = backendUrl.replace(/\/+$/, '') + '/file';
    const params = `path=${encodeURIComponent(path || '/')}`;

    request.request(
      url + '?' + params,
      {
        method: http.RequestMethod.GET,
        header: this.getHeaders(backendUrl, authToken, directory),
        connectTimeout: 5000,
        readTimeout: 5000,
      },
      (err, data) => {
        onSearching(false);
        onRequestChange(null);
        if (!err && data.responseCode === 200) {
          try {
            const parsed = JSON.parse(data.result as string) as FileNode[];
            if (Array.isArray(parsed)) {
              const results = parsed
                .filter((item: FileNode) => item.type === 'directory' && !item.ignored)
                .map((item: FileNode): FileNode => {
                  let absolute = item.absolute;
                  if (!absolute.startsWith('/')) {
                    const parentPath = path.endsWith('/') ? path : path + '/';
                    absolute = parentPath + (item.name || item.path || '');
                  }
                  return {
                    name: item.name,
                    path: item.path,
                    type: item.type,
                    ignored: item.ignored,
                    absolute: absolute
                  };
                });
              onResult(results);
            }
          } catch {}
        } else {
          onResult([]);
        }
      }
    );
  }

  schedulePathSearch(
    query: string,
    onListDir: (path: string) => void,
    onSearch: (query: string, fullPath: string) => void,
    currentTimer: number,
    onTimerChange: (timer: number) => void
  ): void {
    if (currentTimer !== -1) {
      clearTimeout(currentTimer);
    }

    if (!query || query === '/') {
      onListDir('/');
      return;
    }

    const timer = setTimeout(() => {
      const lastSlashIndex = query.lastIndexOf('/');

      if (query.endsWith('/')) {
        onListDir(query);
      } else {
        if (lastSlashIndex !== -1) {
          const searchPart = query.substring(lastSlashIndex + 1);
          if (!searchPart) {
            onListDir(query);
          } else {
            onSearch(searchPart, query);
          }
        } else {
          onSearch(query, query);
        }
      }
    }, 400) as number;

    onTimerChange(timer);
  }

  addRecentPath(path: string, currentRecent: string[]): string[] {
    const normalized = path.replace(/\/+$/, '') || '/';
    const existing = currentRecent.filter(p => p !== normalized);
    const updated = [normalized, ...existing].slice(0, 10);
    AppStorage.SetOrCreate<string[]>('recentWorktreePaths', updated);
    return updated;
  }

  async createOrUpdateSession(
    isEditing: boolean,
    editingId: string,
    title: string,
    directory: string,
    backendUrl: string,
    authToken: string,
    backendId: string,
    selectedRemoteSessionId: string
  ): Promise<boolean> {
    const sessionTitle = title || `${directory.split('/').pop() || directory}`;

    if (isEditing && editingId) {
      this.core.updateProject(editingId, sessionTitle, backendUrl, authToken, directory, '', backendId);
      if (selectedRemoteSessionId) {
        this.core.updateRemoteSessionId(editingId, selectedRemoteSessionId);
      }
      console.info('[SessionDetailViewModel] Local session updated:', editingId);
      AppStorage.SetOrCreate<string>('refreshSessionsNow', Date.now().toString());
      return true;
    } else {
      const newProjectId = await this.core.addProject(sessionTitle, backendUrl, authToken, directory, '', backendId);
      if (selectedRemoteSessionId && newProjectId) {
        this.core.updateRemoteSessionId(newProjectId, selectedRemoteSessionId);
      }
      console.info('[SessionDetailViewModel] Local session created for directory:', directory);
      AppStorage.SetOrCreate<string>('refreshSessionsNow', Date.now().toString());
      return true;
    }
  }

  private getHeaders(backendUrl: string, authToken: string, directory: string): Record<string, string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'x-opencode-directory': encodeURIComponent(directory || '/')
    };
    if (authToken) {
      headers['Authorization'] = 'Basic ' + this.base64Encode('opencode:' + authToken);
    }
    return headers;
  }

  private base64Encode(str: string): string {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    let result = '';
    let i = 0;
    const len = str.length;
    while (i < len) {
      const b1 = str.charCodeAt(i);
      i++;
      const b2 = i < len ? str.charCodeAt(i) : NaN;
      if (i < len) i++;
      const b3 = i < len ? str.charCodeAt(i) : NaN;
      if (i < len) i++;
      result += chars.charAt(b1 >> 2);
      result += chars.charAt(((b1 & 0x03) << 4) | (isNaN(b2) ? 0 : b2 >> 4));
      result += isNaN(b2) ? '=' : chars.charAt(((b2 & 0x0f) << 2) | (isNaN(b3) ? 0 : b3 >> 6));
      result += isNaN(b3) ? '=' : chars.charAt(b3 & 0x3f);
    }
    return result;
  }

  private cancelRequest(request: http.HttpRequest | null, onChange: (req: http.HttpRequest | null) => void): void {
    if (request) {
      request.destroy();
      onChange(null);
    }
  }
}
