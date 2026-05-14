import http from '@ohos.net.http';

export interface OpenCodeSession {
  id: string;
  slug?: string;
  projectID: string;
  directory: string;
  parentID?: string;
  summary?: {
    additions: number;
    deletions: number;
    files: number;
  };
  title: string;
  version: string;
  time: {
    created: number;
    updated: number;
  };
}

export interface OpenCodeMessagePart {
  id: string;
  sessionID: string;
  messageID: string;
  type: 'text' | 'tool' | 'file' | 'reasoning' | 'agent' | 'step-start' | 'step-finish' | 'snapshot' | 'patch' | 'retry' | 'compaction' | 'subtask';
  text?: string;
  tool?: string;
  callID?: string;
  status?: 'pending' | 'running' | 'completed' | 'error';
  state?: unknown;
  metadata?: Record<string, unknown>;
  time?: {
    start: number;
    end?: number;
  };
}

export interface OpenCodeMessage {
  info: {
    id: string;
    sessionID: string;
    role: 'user' | 'assistant';
    agent?: string;
    model?: {
      providerID: string;
      modelID: string;
    };
    time: {
      created: number;
      completed?: number;
    };
    error?: {
      name: string;
      data: Record<string, unknown>;
    };
  };
  parts: OpenCodeMessagePart[];
}

export interface TextPartInput {
  type: 'text';
  text: string;
}

export class OpenCodeApiClient {
  private baseUrl: string = '';
  private authToken: string = '';
  private directory: string = '';
  private currentRequest: http.HttpRequest | null = null;

  constructor(baseUrl: string = '', authToken: string = '', directory: string = '') {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.authToken = authToken;
    this.directory = directory;
  }

  updateConfig(baseUrl: string, authToken: string, directory: string) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.authToken = authToken;
    this.directory = directory;
  }

  private getHeaders(): Record<string, string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'x-opencode-directory': encodeURIComponent(this.directory)
    };
    if (this.authToken) {
      headers['Authorization'] = 'Basic ' + this.base64Encode('opencode:' + this.authToken);
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

  async listSessions(): Promise<OpenCodeSession[]> {
    if (!this.baseUrl) return [];
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = this.baseUrl + '/experimental/session';
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    if (this.authToken) {
      headers['Authorization'] = 'Basic ' + this.base64Encode('opencode:' + this.authToken);
    }
    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.GET,
            header: headers,
            connectTimeout: 10000,
            readTimeout: 10000,
          },
          (err, data) => {
            if (err) reject(err);
            else resolve(data);
          }
        );
      });
      if (result.responseCode === 200) {
        const res = JSON.parse(result.result as string) as OpenCodeSession[];
        return res.map(s => ({
          ...s,
          title: `[${s.slug || s.id}] ${s.title || s.slug || '未命名会话'}`
        }));
      }
      return [];
    } catch (e) {
      console.error('[OpenCodeApiClient] listSessions error:', e);
      return [];
    } finally {
      this.cancelRequest();
    }
  }

  async createSession(title?: string, parentID?: string): Promise<OpenCodeSession | null> {
    if (!this.baseUrl) return null;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = this.baseUrl + '/session';
    const body: Record<string, string> = {};
    if (title) body['title'] = title;
    if (parentID) body['parentID'] = parentID;
    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.POST,
            header: this.getHeaders(),
            extraData: JSON.stringify(body),
            connectTimeout: 10000,
            readTimeout: 10000,
          },
          (err, data) => {
            if (err) reject(err);
            else resolve(data);
          }
        );
      });
      if (result.responseCode === 200) {
        return JSON.parse(result.result as string) as OpenCodeSession;
      }
      return null;
    } catch (e) {
      console.error('[OpenCodeApiClient] createSession error:', e);
      return null;
    } finally {
      this.cancelRequest();
    }
  }

  async getSession(sessionID: string): Promise<OpenCodeSession | null> {
    if (!this.baseUrl) return null;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = `${this.baseUrl}/session/${encodeURIComponent(sessionID)}`;
    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.GET,
            header: this.getHeaders(),
            connectTimeout: 10000,
            readTimeout: 10000,
          },
          (err, data) => {
            if (err) reject(err);
            else resolve(data);
          }
        );
      });
      if (result.responseCode === 200) {
        return JSON.parse(result.result as string) as OpenCodeSession;
      }
      return null;
    } catch (e) {
      console.error('[OpenCodeApiClient] getSession error:', e);
      return null;
    } finally {
      this.cancelRequest();
    }
  }

  async deleteSession(sessionID: string): Promise<boolean> {
    if (!this.baseUrl) return false;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = `${this.baseUrl}/session/${encodeURIComponent(sessionID)}`;
    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.DELETE,
            header: this.getHeaders(),
            connectTimeout: 10000,
            readTimeout: 10000,
          },
          (err, data) => {
            if (err) reject(err);
            else resolve(data);
          }
        );
      });
      return result.responseCode === 200;
    } catch (e) {
      console.error('[OpenCodeApiClient] deleteSession error:', e);
      return false;
    } finally {
      this.cancelRequest();
    }
  }

  async getMessages(sessionID: string, limit?: number): Promise<OpenCodeMessage[]> {
    if (!this.baseUrl) return [];
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    let url = `${this.baseUrl}/session/${encodeURIComponent(sessionID)}/message`;
    if (limit) {
      url += `?limit=${limit}`;
    }
    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.GET,
            header: this.getHeaders(),
            connectTimeout: 10000,
            readTimeout: 10000,
          },
          (err, data) => {
            if (err) reject(err);
            else resolve(data);
          }
        );
      });
      if (result.responseCode === 200) {
        return JSON.parse(result.result as string) as OpenCodeMessage[];
      }
      return [];
    } catch (e) {
      console.error('[OpenCodeApiClient] getMessages error:', e);
      return [];
    } finally {
      this.cancelRequest();
    }
  }

  async sendPrompt(sessionID: string, parts: TextPartInput[]): Promise<OpenCodeMessage | null> {
    if (!this.baseUrl) return null;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = `${this.baseUrl}/session/${encodeURIComponent(sessionID)}/message`;
    const body = { parts: parts };
    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.POST,
            header: this.getHeaders(),
            extraData: JSON.stringify(body),
            connectTimeout: 60000,
            readTimeout: 60000,
          },
          (err, data) => {
            if (err) reject(err);
            else resolve(data);
          }
        );
      });
      if (result.responseCode === 200) {
        return JSON.parse(result.result as string) as OpenCodeMessage;
      }
      return null;
    } catch (e) {
      console.error('[OpenCodeApiClient] sendPrompt error:', e);
      return null;
    } finally {
      this.cancelRequest();
    }
  }

  async promptAsync(sessionID: string, parts: TextPartInput[]): Promise<boolean> {
    if (!this.baseUrl) return false;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = `${this.baseUrl}/session/${encodeURIComponent(sessionID)}/prompt_async`;
    const body = { parts: parts };
    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.POST,
            header: this.getHeaders(),
            extraData: JSON.stringify(body),
            connectTimeout: 10000,
            readTimeout: 10000,
          },
          (err, data) => {
            if (err) reject(err);
            else resolve(data);
          }
        );
      });
      return result.responseCode === 204;
    } catch (e) {
      console.error('[OpenCodeApiClient] promptAsync error:', e);
      return false;
    } finally {
      this.cancelRequest();
    }
  }

  async abortSession(sessionID: string): Promise<boolean> {
    if (!this.baseUrl) return false;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = `${this.baseUrl}/session/${encodeURIComponent(sessionID)}/abort`;
    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.POST,
            header: this.getHeaders(),
            connectTimeout: 10000,
            readTimeout: 10000,
          },
          (err, data) => {
            if (err) reject(err);
            else resolve(data);
          }
        );
      });
      return result.responseCode === 200 || result.responseCode === 204;
    } catch (e) {
      console.error('[OpenCodeApiClient] abortSession error:', e);
      return false;
    } finally {
      this.cancelRequest();
    }
  }

  private cancelRequest() {
    if (this.currentRequest) {
      this.currentRequest.destroy();
      this.currentRequest = null;
    }
  }
}
