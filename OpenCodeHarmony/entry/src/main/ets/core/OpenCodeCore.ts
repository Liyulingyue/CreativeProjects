import dataPreferences from '@ohos.data.preferences';
import common from '@ohos.app.ability.common';
import http from '@ohos.net.http';

export interface OpenCodeSession {
  id: string;
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
    const url = this.baseUrl + '/session';
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
            if (err) {
              reject(err);
            } else {
              resolve(data);
            }
          }
        );
      });
      if (result.responseCode === 200) {
        return JSON.parse(result.result as string) as OpenCodeSession[];
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
    const body = {
      parts: parts
    };
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
    const body = {
      parts: parts
    };
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

export interface OpenCodeProject {
  id: string;
  name: string;
  url: string;
  authToken: string;
  path: string;
  notes: string;
  backendId: string;
  lastAccess: number;
}

export interface ChatSession {
  id: string;
  name: string;
  backendUrl: string;
  directory: string;
  updatedAt: string;
}

export interface OpenCodeBackend {
  id: string;
  url: string;
  authToken: string;
  notes: string;
}

export class OpenCodeCore {
  private static instance: OpenCodeCore;
  private projects: OpenCodeProject[] = [];
  private backends: OpenCodeBackend[] = [];
  private currentProjectId: string = '';
  private currentSessionId: string = '';
  private preferences: dataPreferences.Preferences | null = null;
  private context: common.UIAbilityContext | null = null;
  private apiClient: OpenCodeApiClient = new OpenCodeApiClient();
  private static readonly PREF_NAME = 'opencode_data';
  private static readonly KEY_BACKENDS = 'backends';
  private static readonly KEY_PROJECTS = 'projects';

  private constructor() {}

  public static getInstance(): OpenCodeCore {
    if (!OpenCodeCore.instance) {
      OpenCodeCore.instance = new OpenCodeCore();
    }
    return OpenCodeCore.instance;
  }

  public async init(context: common.UIAbilityContext): Promise<void> {
    this.context = context;
    try {
      this.preferences = await dataPreferences.getPreferences(context, OpenCodeCore.PREF_NAME);
      await this.loadFromStorage();
      console.info('[OpenCodeCore] Persistence initialized, backends:', this.backends.length);
    } catch (err) {
      console.error('[OpenCodeCore] Failed to init preferences:', err);
    }
  }

  private async loadFromStorage(): Promise<void> {
    if (!this.preferences) return;

    try {
      const backendsJson = await this.preferences.get(OpenCodeCore.KEY_BACKENDS, '[]') as string;
      this.backends = JSON.parse(backendsJson) as OpenCodeBackend[];

      const projectsJson = await this.preferences.get(OpenCodeCore.KEY_PROJECTS, '[]') as string;
      this.projects = JSON.parse(projectsJson) as OpenCodeProject[];
    } catch (err) {
      console.error('[OpenCodeCore] Failed to load from storage:', err);
      this.backends = [];
      this.projects = [];
    }
  }

  private async saveBackends(): Promise<void> {
    if (!this.preferences) return;

    try {
      await this.preferences.put(OpenCodeCore.KEY_BACKENDS, JSON.stringify(this.backends));
      await this.preferences.flush();
    } catch (err) {
      console.error('[OpenCodeCore] Failed to save backends:', err);
    }
  }

  private async saveProjects(): Promise<void> {
    if (!this.preferences) return;

    try {
      await this.preferences.put(OpenCodeCore.KEY_PROJECTS, JSON.stringify(this.projects));
      await this.preferences.flush();
    } catch (err) {
      console.error('[OpenCodeCore] Failed to save projects:', err);
    }
  }

  public getProjects(): OpenCodeProject[] {
    return this.projects;
  }

  public async addProject(name: string, url: string, authToken: string, path: string, notes: string = '', backendId: string = ''): Promise<void> {
    const newProject: OpenCodeProject = {
      id: Date.now().toString(),
      name: name,
      url: url,
      authToken: authToken,
      path: path,
      notes: notes,
      backendId: backendId,
      lastAccess: Date.now()
    };
    this.projects.push(newProject);
    await this.saveProjects();
  }

  public addProjectWithBackend(name: string, backendUrl: string, backendAuthToken: string, path: string, notes: string = ''): void {
    const backend = this.backends.find(b => b.url === backendUrl);
    this.addProject(name, backendUrl, backendAuthToken, path, notes, backend?.id ?? '');
  }

  public getProjectById(id: string): OpenCodeProject | undefined {
    return this.projects.find(p => p.id === id);
  }

  public updateProject(id: string, name: string, url: string, authToken: string, path: string, notes: string = '', backendId: string = ''): void {
    const index = this.projects.findIndex(p => p.id === id);
    if (index !== -1) {
      const backend = this.backends.find(b => b.url === url);
      this.projects[index] = {
        ...this.projects[index],
        name,
        url,
        authToken,
        path,
        notes,
        backendId: backendId || (backend?.id ?? this.projects[index].backendId),
        lastAccess: Date.now()
      };
      this.saveProjects();
    }
  }

  public removeProject(id: string): void {
    this.projects = this.projects.filter(p => p.id !== id);
    this.saveProjects();
  }

  public setCurrentProject(id: string): void {
    this.currentProjectId = id;
    const project = this.projects.find(p => p.id === id);
    if (project) {
      project.lastAccess = Date.now();
      this.apiClient.updateConfig(project.url, project.authToken, project.path);
    }
  }

  public getCurrentProject(): OpenCodeProject | undefined {
    return this.projects.find(p => p.id === this.currentProjectId);
  }

  public setCurrentSession(sessionId: string): void {
    this.currentSessionId = sessionId;
  }

  public getCurrentSessionId(): string {
    return this.currentSessionId;
  }

  public getBackendUrl(): string {
    const project = this.getCurrentProject();
    return project ? project.url : '';
  }

  public async storeBackendUrl(url: string): Promise<void> {
    console.info(`[OpenCodeCore] Backend URL updated from JS: ${url}`);
    if (this.projects.length === 0) {
      await this.addProject('Default Project', url, '', '/', '', '');
    }
  }

  public async sendCommand(command: string): Promise<{ status: string; timestamp: number; message?: string }> {
    const project = this.getCurrentProject();
    if (!project) {
      throw new Error('No project selected');
    }
    const sessionId = this.getCurrentSessionId();
    if (!sessionId) {
      throw new Error('No session selected');
    }
    console.info(`[OpenCodeCore] Executing: ${command} on ${project.url}`);
    const result = await this.apiClient.sendPrompt(sessionId, [{ type: 'text', text: command }]);
    if (result) {
      const textParts = result.parts.filter(p => p.type === 'text').map(p => p.text).join('\n');
      return { status: 'success', timestamp: Date.now(), message: textParts || 'Command executed' };
    }
    return { status: 'error', timestamp: Date.now(), message: 'Failed to execute command' };
  }

  public getInjectedScripts(): string {
    return `console.log("OpenCode Mobile Native Bridge Active");`;
  }

  public getBackends(): OpenCodeBackend[] {
    return this.backends;
  }

  public async addBackend(url: string, authToken: string, notes: string): Promise<void> {
    this.backends.push({
      id: Date.now().toString(),
      url,
      authToken,
      notes,
    });
    await this.saveBackends();
  }

  public async removeBackend(id: string): Promise<void> {
    this.backends = this.backends.filter(b => b.id !== id);
    await this.saveBackends();
  }

  public async updateBackend(id: string, url: string, authToken: string, notes: string): Promise<void> {
    const index = this.backends.findIndex(b => b.id === id);
    if (index !== -1) {
      this.backends[index] = { ...this.backends[index], url, authToken, notes };
      await this.saveBackends();
    }
  }

  public getBackendById(id: string): OpenCodeBackend | undefined {
    return this.backends.find(b => b.id === id);
  }

  public getSessions(): ChatSession[] {
    return this.projects.map(p => ({
      id: p.id,
      name: p.name,
      backendUrl: p.url,
      directory: p.path,
      updatedAt: new Date(p.lastAccess).toLocaleString()
    }));
  }

  public async updateSession(id: string, name: string, backendUrl: string, directory: string): Promise<void> {
    this.updateProject(id, name, backendUrl, '', directory, '');
  }

  public async removeSession(id: string): Promise<void> {
    this.removeProject(id);
  }

  public async refreshSessionsFromBackend(backendUrl: string, authToken: string, directory: string): Promise<OpenCodeSession[]> {
    this.apiClient.updateConfig(backendUrl, authToken, directory);
    const sessions = await this.apiClient.listSessions();
    return sessions;
  }

  public async createBackendSession(backendUrl: string, authToken: string, directory: string, title?: string): Promise<OpenCodeSession | null> {
    this.apiClient.updateConfig(backendUrl, authToken, directory);
    return await this.apiClient.createSession(title);
  }

  public async deleteBackendSession(sessionId: string): Promise<boolean> {
    return await this.apiClient.deleteSession(sessionId);
  }

  public async getBackendMessages(sessionId: string): Promise<OpenCodeMessage[]> {
    return await this.apiClient.getMessages(sessionId);
  }

  public async sendBackendPrompt(sessionId: string, text: string): Promise<OpenCodeMessage | null> {
    return await this.apiClient.sendPrompt(sessionId, [{ type: 'text', text }]);
  }

  public async abortBackendSession(sessionId: string): Promise<boolean> {
    return await this.apiClient.abortSession(sessionId);
  }
}
