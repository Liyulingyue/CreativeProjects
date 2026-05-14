import dataPreferences from '@ohos.data.preferences';
import common from '@ohos.app.ability.common';

/**
 * 项目定义
 */
export interface OpenCodeProject {
  id: string;
  name: string;        // 项目名称
  url: string;         // 后端服务器地址 (http://ip:port)
  authToken: string;   // 鉴权 Token
  path: string;        // 项目在服务端的绝对路径
  notes: string;       // 备注
  backendId: string;   // 关联的后端 ID
  lastAccess: number;  // 最后访问时间
}

/**
 * 会话定义（复用 OpenCodeProject）
 */
export interface ChatSession {
  id: string;
  name: string;
  backendUrl: string;
  directory: string;
  updatedAt: string;
}

/**
 * 后端连接定义
 */
export interface OpenCodeBackend {
  id: string;
  url: string;         // 后端地址 (http://ip:port)
  authToken: string;   // 鉴权密码/Token
  notes: string;       // 备注
}

/**
 * OpenCodeCore - 核心业务逻辑层
 * 负责多项目管理、数据持久化及与后端的 API 交互
 */
export class OpenCodeCore {
  private static instance: OpenCodeCore;
  private projects: OpenCodeProject[] = [];
  private backends: OpenCodeBackend[] = [];
  private currentProjectId: string = '';
  private preferences: dataPreferences.Preferences | null = null;
  private context: common.UIAbilityContext | null = null;
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

  /**
   * 初始化持久化存储（应用启动时调用）
   */
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

  /**
   * 从持久化存储加载数据
   */
  private async loadFromStorage(): Promise<void> {
    if (!this.preferences) return;

    try {
      // 加载后端列表
      const backendsJson = await this.preferences.get(OpenCodeCore.KEY_BACKENDS, '[]') as string;
      this.backends = JSON.parse(backendsJson) as OpenCodeBackend[];

      // 加载项目列表
      const projectsJson = await this.preferences.get(OpenCodeCore.KEY_PROJECTS, '[]') as string;
      this.projects = JSON.parse(projectsJson) as OpenCodeProject[];
    } catch (err) {
      console.error('[OpenCodeCore] Failed to load from storage:', err);
      this.backends = [];
      this.projects = [];
    }
  }

  /**
   * 保存后端列表到持久化存储
   */
  private async saveBackends(): Promise<void> {
    if (!this.preferences) return;

    try {
      await this.preferences.put(OpenCodeCore.KEY_BACKENDS, JSON.stringify(this.backends));
      await this.preferences.flush();
    } catch (err) {
      console.error('[OpenCodeCore] Failed to save backends:', err);
    }
  }

  /**
   * 保存项目列表到持久化存储
   */
  private async saveProjects(): Promise<void> {
    if (!this.preferences) return;

    try {
      await this.preferences.put(OpenCodeCore.KEY_PROJECTS, JSON.stringify(this.projects));
      await this.preferences.flush();
    } catch (err) {
      console.error('[OpenCodeCore] Failed to save projects:', err);
    }
  }

  /**
   * 获取所有项目列表
   */
  public getProjects(): OpenCodeProject[] {
    return this.projects;
  }

  /**
   * 添加新项目
   */
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

  /**
   * 设置当前激活的项目
   */
  public setCurrentProject(id: string): void {
    this.currentProjectId = id;
    const project = this.projects.find(p => p.id === id);
    if (project) {
      project.lastAccess = Date.now();
    }
  }

  public getCurrentProject(): OpenCodeProject | undefined {
    return this.projects.find(p => p.id === this.currentProjectId);
  }

  /**
   * 获取当前后端地址 (对接已有的 Web 逻辑)
   */
  public getBackendUrl(): string {
    const project = this.getCurrentProject();
    return project ? project.url : '';
  }

  /**
   * 给 JS 调用的接口：存储后端地址 (兼容旧逻辑)
   */
  public async storeBackendUrl(url: string): Promise<void> {
    console.info(`[OpenCodeCore] Backend URL updated from JS: ${url}`);
    // 兼容逻辑：如果没有项目，创建一个默认项目
    if (this.projects.length === 0) {
      await this.addProject('Default Project', url, '', '/', '', '');
    }
  }

  /**
   * 发送指令到 OpenCode 后端 (原生 API 实现)
   */
  public async sendCommand(command: string): Promise<{ status: string; timestamp: number }> {
    const project = this.getCurrentProject();
    if (!project) {
      throw new Error('No project selected');
    }
    console.info(`[OpenCodeCore] Executing: ${command} on ${project.url}`);
    // TODO: 使用 @ohos.net.http 发起真实请求
    return { status: 'success', timestamp: Date.now() };
  }

  public getInjectedScripts(): string {
    return `console.log("OpenCode Mobile Native Bridge Active");`;
  }

  // ── 后端管理 ──────────────────────────────────────────

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

  // ── 会话管理（基于 Project）─────────────────────────────────

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
}
