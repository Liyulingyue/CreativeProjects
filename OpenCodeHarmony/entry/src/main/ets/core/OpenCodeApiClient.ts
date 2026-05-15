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

export interface OpenCodeProviderModel {
  id: string;
  modelID?: string;
  name: string;
  family: string;
  status: string;
  providerID?: string;
  providerName?: string; // 扩充字段，用于在选择列表中显示所属提供者
}

export interface OpenCodeProvider {
  id: string;
  name: string;
  models: Record<string, OpenCodeProviderModel>;
}

export interface OpenCodeProvidersResponse {
  all: OpenCodeProvider[];
  default: Record<string, string>;
  connected: string[];
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
      data: Record<string, Object>;
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

  async listModels(): Promise<OpenCodeProviderModel[]> {
    if (!this.baseUrl) return [];
    try {
      const url = this.baseUrl + '/provider';
      const headers: Record<string, string> = { 'Content-Type': 'application/json' };
      if (this.authToken) {
        headers['Authorization'] = 'Basic ' + this.base64Encode('opencode:' + this.authToken);
      }

      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        const req = http.createHttp();
        req.request(url, { method: http.RequestMethod.GET, header: headers }, (err, data) => {
          if (err) reject(err); else resolve(data);
        });
      });

      if (result.responseCode === 200) {
        const res = JSON.parse(result.result as string) as OpenCodeProvidersResponse;
        const allModels: OpenCodeProviderModel[] = [];
        
        // 如果有 connected 字段，我们可以优先展示这些 provider 的模型
        const connectedProviders = res.connected || [];
        
        if (res.all) {
          res.all.forEach(p => {
            // 只添加已连接的 provider 的模型，或者如果没有 connected 列表则添加全部
            if (connectedProviders.length === 0 || connectedProviders.includes(p.id)) {
              if (p.models) {
                Object.entries(p.models).forEach(([modelKey, m]) => {
                  // 注入 provider id/name，便于前端保存稳定的 provider/model 组合
                  m.providerID = p.id;
                  // 使用 provider.models 的 key 作为后端稳定 modelID（避免仅靠 m.id 产生歧义）
                  m.modelID = modelKey;
                  // 注入 provider 的显示名称，方便 UI 展示
                  m.providerName = p.name;
                  allModels.push(m);
                });
              }
            }
          });
        }
        return allModels;
      }
      return [];
    } catch (e) {
      console.error('[OpenCodeApiClient] listModels error:', e);
      return [];
    }
  }

  async createSession(title?: string, parentID?: string, preferredModel?: string): Promise<OpenCodeSession | null> {
    if (!this.baseUrl) return null;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = this.baseUrl + '/session';
    const body: Record<string, string> = {};
    if (title) body['title'] = title;
    if (parentID) body['parentID'] = parentID;
    // 暂停在 createSession 透传字符串模型，避免与后端结构化 model 协议不一致。
    // 当前模型选择在 prompt/message 阶段通过结构化 model 对象下发。
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

  async sendPrompt(sessionID: string, parts: TextPartInput[], model?: string): Promise<OpenCodeMessage | null> {
    if (!this.baseUrl) return null;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = `${this.baseUrl}/session/${encodeURIComponent(sessionID)}/message`;
    const body: Record<string, any> = { parts: parts };
    if (model && model.includes('/')) {
      const parts = model.split('/');
      body['model'] = {
        providerID: parts[0],
        modelID: parts[1]
      };
    }
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

  async promptAsync(sessionID: string, parts: TextPartInput[], model?: string): Promise<boolean> {
    if (!this.baseUrl) return false;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = `${this.baseUrl}/session/${encodeURIComponent(sessionID)}/prompt_async`;
    const body: Record<string, any> = { parts: parts };
    if (model && model.includes('/')) {
      const parts = model.split('/');
      body['model'] = {
        providerID: parts[0],
        modelID: parts[1]
      };
    }
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
