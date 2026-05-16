import dataPreferences from '@ohos.data.preferences';
import common from '@ohos.app.ability.common';
import { OpenCodeApiClient, OpenCodeSession, OpenCodeMessage } from './OpenCodeApiClient';

export { OpenCodeSession, OpenCodeMessage, OpenCodeProviderModel } from './OpenCodeApiClient';
export type { OpenCodeApiClient, TextPartInput, OpenCodeMessagePart } from './OpenCodeApiClient';

export interface OpenCodeProject {
  id: string;
  name: string;
  url: string;
  username: string;
  authToken: string;
  path: string;
  notes: string;
  backendId: string;
  remoteSessionId?: string;
  preferredModel?: string;
  lastAccess: number;
}

export interface ChatSession {
  id: string;
  name: string;
  backendUrl: string;
  backendId: string;
  directory: string;
  remoteSessionId?: string;
  updatedAt: string;
}

export interface OpenCodeBackend {
  id: string;
  url: string;
  username: string;
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

  public async addProject(name: string, url: string, username: string, authToken: string, path: string, notes: string = '', backendId: string = ''): Promise<string> {
    const newProject: OpenCodeProject = {
      id: Date.now().toString(),
      name: name,
      url: url,
      username: username,
      authToken: authToken,
      path: path,
      notes: notes,
      backendId: backendId,
      lastAccess: Date.now()
    };
    this.projects.push(newProject);
    await this.saveProjects();
    AppStorage.SetOrCreate<string>('needRefreshSessions', 'true');
    return newProject.id;
  }

  public addProjectWithBackend(name: string, backendUrl: string, backendUsername: string, backendAuthToken: string, path: string, notes: string = ''): void {
    const backend = this.backends.find(b => b.url === backendUrl);
    this.addProject(name, backendUrl, backendUsername, backendAuthToken, path, notes, backend?.id ?? '');
  }

  public getProjectById(id: string): OpenCodeProject | undefined {
    return this.projects.find(p => p.id === id);
  }

  public updateProject(id: string, name: string, url: string, username: string, authToken: string, path: string, notes: string = '', backendId: string = '', preferredModel?: string): void {
    const index = this.projects.findIndex(p => p.id === id);
    if (index !== -1) {
      const backend = this.backends.find(b => b.url === url);
      this.projects[index] = {
        ...this.projects[index],
        name,
        url,
        username,
        authToken,
        path,
        notes,
        backendId: backendId || (backend?.id ?? this.projects[index].backendId),
        preferredModel: preferredModel !== undefined ? preferredModel : this.projects[index].preferredModel,
        lastAccess: Date.now()
      };
      this.saveProjects();
    }
  }

  public removeProject(id: string): void {
    this.projects = this.projects.filter(p => p.id !== id);
    this.saveProjects();
    AppStorage.SetOrCreate<string>('needRefreshSessions', 'true');
  }

  public setCurrentProject(id: string): void {
    this.currentProjectId = id;
    const project = this.projects.find(p => p.id === id);
    if (project) {
      project.lastAccess = Date.now();
      this.apiClient.updateConfig(project.url, project.username, project.authToken, project.path);
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
      await this.addProject('Default Project', url, '', '', '/', '', '');
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

  public async addBackend(url: string, username: string, authToken: string, notes: string): Promise<void> {
    this.backends.push({
      id: Date.now().toString(),
      url,
      username,
      authToken,
      notes,
    });
    await this.saveBackends();
  }

  public async removeBackend(id: string): Promise<void> {
    this.backends = this.backends.filter(b => b.id !== id);
    await this.saveBackends();
  }

  public async updateBackend(id: string, url: string, username: string, authToken: string, notes: string): Promise<void> {
    const index = this.backends.findIndex(b => b.id === id);
    if (index !== -1) {
      this.backends[index] = { ...this.backends[index], url, username, authToken, notes };
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
      backendId: p.backendId,
      directory: p.path,
      remoteSessionId: p.remoteSessionId,
      updatedAt: new Date(p.lastAccess).toLocaleString()
    }));
  }

  public async updateRemoteSessionId(projectId: string, remoteId: string): Promise<void> {
    const project = this.projects.find(p => p.id === projectId);
    if (project) {
      project.remoteSessionId = remoteId;
      await this.saveProjects();
    }
  }

  public async updateSession(id: string, name: string, backendUrl: string, directory: string): Promise<void> {
    const project = this.projects.find(p => p.id === id);
    const username = project?.username ?? '';
    this.updateProject(id, name, backendUrl, username, '', directory, '');
  }

  public async removeSession(id: string): Promise<void> {
    this.removeProject(id);
  }

  public async refreshSessionsFromBackend(backendUrl: string, username: string, authToken: string, directory: string): Promise<OpenCodeSession[]> {
    console.info('[OpenCodeCore] refreshSessionsFromBackend:', backendUrl, authToken ? 'with token' : 'no token', directory);
    this.apiClient.updateConfig(backendUrl, username, authToken, directory);
    const sessions = await this.apiClient.listSessions();
    console.info('[OpenCodeCore] listSessions result:', sessions.length);
    return sessions;
  }

  public async createBackendSession(backendUrl: string, username: string, authToken: string, directory: string, title?: string, preferredModel?: string): Promise<OpenCodeSession | null> {
    this.apiClient.updateConfig(backendUrl, username, authToken, directory);
    return await this.apiClient.createSession(title, undefined, preferredModel);
  }

  public async deleteBackendSession(sessionId: string): Promise<boolean> {
    return await this.apiClient.deleteSession(sessionId);
  }

  public async getBackendMessages(sessionId: string): Promise<OpenCodeMessage[]> {
    return await this.apiClient.getMessages(sessionId);
  }

  public async sendBackendPrompt(sessionId: string, text: string, model?: string): Promise<OpenCodeMessage | null> {
    return await this.apiClient.sendPrompt(sessionId, [{ type: 'text', text }], model);
  }

  public async abortBackendSession(sessionId: string): Promise<boolean> {
    return await this.apiClient.abortSession(sessionId);
  }
}
