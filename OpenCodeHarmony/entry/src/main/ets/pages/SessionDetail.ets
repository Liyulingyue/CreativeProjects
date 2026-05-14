import { OpenCodeCore, OpenCodeBackend } from '../core/OpenCodeCore';
import router from '@ohos.router';
import http from '@ohos.net.http';

interface FileNode {
  name: string;
  path: string;
  absolute: string;
  type: 'file' | 'directory';
  ignored: boolean;
}

interface SessionResponse {
  id: string;
}

@Entry
@Component
struct SessionDetail {
  @State title: string = '';
  @State directory: string = '/';
  @State isCreating: boolean = false;
  @State errorMessage: string = '';
  @State backendId: string = '';
  @State backendUrl: string = '';
  @State authToken: string = '';
  @State backends: OpenCodeBackend[] = [];
  @State showBackendPicker: boolean = false;
  @State recentPaths: string[] = [];
  @State showPathPicker: boolean = false;
  @State pathSearchResults: FileNode[] = [];
  @State isSearchingPath: boolean = false;
  @State isEditing: boolean = false;
  @State editingId: string = '';
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  private currentRequest: http.HttpRequest | null = null;
  private pathSearchTimer: number = -1;

  aboutToAppear() {
    this.backends = this.core.getBackends();
    const params = router.getParams() as Record<string, Object>;
    if (params) {
      this.backendId = (params['backendId'] as string) || '';
      this.backendUrl = (params['backendUrl'] as string) || '';
      this.authToken = (params['authToken'] as string) || '';
      this.isEditing = (params['isEditing'] as boolean) || false;
      this.editingId = (params['id'] as string) || '';
      if (this.isEditing) {
        this.title = (params['title'] as string) || '';
        this.directory = (params['directory'] as string) || '/';
      }
    }
    if (!this.backendId && this.backends.length > 0) {
      this.selectBackend(this.backends[0]);
    }
    this.recentPaths = AppStorage.get<string[]>('recentWorktreePaths') ?? [];
  }

  private selectBackend(backend: OpenCodeBackend) {
    this.backendId = backend.id;
    this.backendUrl = backend.url;
    this.authToken = backend.authToken;
    this.showBackendPicker = false;
  }

  private getSelectedBackend(): OpenCodeBackend | undefined {
    return this.backends.find(b => b.id === this.backendId);
  }

  private getHeaders(): Record<string, string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'x-opencode-directory': encodeURIComponent(this.directory || '/')
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

  private cancelRequest() {
    if (this.currentRequest) {
      this.currentRequest.destroy();
      this.currentRequest = null;
    }
  }

  private addRecentPath(path: string) {
    const normalized = path.replace(/\/+$/, '') || '/';
    const existing = this.recentPaths.filter(p => p !== normalized);
    const updated = [normalized, ...existing].slice(0, 10);
    this.recentPaths = updated;
    AppStorage.SetOrCreate<string[]>('recentWorktreePaths', updated);
  }

  private schedulePathSearch(query: string) {
    if (this.pathSearchTimer !== -1) {
      clearTimeout(this.pathSearchTimer);
    }
    
    // 如果是空或只有一个斜杠，显示根目录内容
    if (!query || query === '/') {
      this.listDirectory('/');
      return;
    }

    // 防抖处理
    this.pathSearchTimer = setTimeout(() => {
      const lastSlashIndex = query.lastIndexOf('/');
      
      // 检查路径结尾是否是斜杠
      if (query.endsWith('/')) {
        // 用户输入了斜杠，列出该目录下的直接子目录
        this.listDirectory(query);
      } else {
        // 用户正在输入目录名片段
        if (lastSlashIndex !== -1) {
          // 提取当前正在输入的片段进行模糊搜索，并传入全路径用于匹配过滤
          const searchPart = query.substring(lastSlashIndex + 1);
          // 哪怕 searchPart 为空（例如刚打完斜杠但 timer 触发），也按目录列出逻辑走
          if (!searchPart) {
            this.listDirectory(query);
          } else {
            this.searchPaths(searchPart, query);
          }
        } else {
          // 纯文字输入（非路径格式），直接搜索
          this.searchPaths(query, query);
        }
      }
    }, 400) as number;
  }

  private searchPaths(query: string, fullPath?: string) {
    if (!this.backendUrl || this.backendUrl === 'http://') {
      this.isSearchingPath = false;
      return;
    }
    this.isSearchingPath = true;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = this.backendUrl.replace(/\/+$/, '') + '/find/file';
    
    // 增加探索深度和结果数量
    const params = `query=${encodeURIComponent(query)}&type=directory&limit=50`;
    try {
      this.currentRequest.request(
        url + '?' + params,
        {
          method: http.RequestMethod.GET,
          header: this.getHeaders(),
          connectTimeout: 5000,
          readTimeout: 5000,
        },
        (err, data) => {
          this.isSearchingPath = false;
          this.currentRequest = null;
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

                // 严格遵循 OpenCode Web 逻辑：
                if (fullPath) {
                  const filterPath = fullPath.toLowerCase();
                  
                  results = results.filter(r => {
                    const rAbs = r.absolute.toLowerCase();
                    
                    // 必须以当前输入作为前缀
                    if (!rAbs.startsWith(filterPath)) return false;
                    
                    // 排除掉输入路径本身（例如输入 /home/，不显示 /home/）
                    if (rAbs === filterPath || rAbs === filterPath + '/') return false;

                    // 计算剩余路径部分
                    let remaining = rAbs.substring(filterPath.length);
                    // 移除开头可能存在的斜杠
                    if (remaining.startsWith('/')) {
                      remaining = remaining.substring(1);
                    }
                    
                    // 核心逻辑：如果剩余部分还包含斜杠（且不是末尾斜杠），说明太深了
                    const lastSlash = remaining.lastIndexOf('/');
                    if (lastSlash !== -1 && lastSlash !== remaining.length - 1) {
                      return false;
                    }
                    return true;
                  });
                }

                // 排序：按字母顺序
                results.sort((a, b) => a.absolute.localeCompare(b.absolute));

                this.pathSearchResults = results;
              }
            } catch (e) {
              console.error('[SessionDetail] Parse search error:', e);
            }
          }
        }
      );
    } catch (e) {
      this.isSearchingPath = false;
    }
  }

  private listDirectory(path: string) {
    if (!this.backendUrl || this.backendUrl === 'http://') {
      this.isSearchingPath = false;
      return;
    }
    this.isSearchingPath = true;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = this.backendUrl.replace(/\/+$/, '') + '/file';
    const params = `path=${encodeURIComponent(path || '/')}`;
    try {
      this.currentRequest.request(
        url + '?' + params,
        {
          method: http.RequestMethod.GET,
          header: this.getHeaders(),
          connectTimeout: 5000,
          readTimeout: 5000,
        },
        (err, data) => {
          this.isSearchingPath = false;
          this.currentRequest = null;
          if (!err && data.responseCode === 200) {
            try {
              const parsed = JSON.parse(data.result as string) as FileNode[];
              if (Array.isArray(parsed)) {
                this.pathSearchResults = parsed
                  .filter((item: FileNode) => item.type === 'directory' && !item.ignored)
                  .map((item: FileNode): FileNode => {
                    // 核心修复：后端 /file 如果返回相对路径，则拼接到当前路径
                    // 如果后端已经返回了以 / 开头的绝对路径，则直接使用
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
              }
            } catch {}
          }
        }
      );
    } catch {}
  }

  private async createSession(): Promise<boolean> {
    if (!this.backendUrl || this.isCreating) return false;

    this.isCreating = true;
    this.errorMessage = '';
    
    try {
      const sessionTitle = this.title || `${this.directory.split('/').pop() || this.directory}`;
      
      if (this.isEditing && this.editingId) {
        // 更新现有本地会话
        this.core.updateProject(
          this.editingId,
          sessionTitle,
          this.backendUrl,
          this.authToken,
          this.directory,
          '', // notes
          this.backendId
        );
        console.info('[SessionDetail] Local session updated:', this.editingId);
      } else {
        // 创建新本地会话
        await this.core.addProject(
          sessionTitle,
          this.backendUrl,
          this.authToken,
          this.directory,
          '', // notes
          this.backendId
        );
        console.info('[SessionDetail] Local session created for directory:', this.directory);
      }

      if (this.directory) {
        this.addRecentPath(this.directory);
      }
      
      return true;
    } catch (e) {
      this.errorMessage = `${this.isEditing ? '更新' : '创建'}失败: ${e}`;
      console.error('[SessionDetail] Create/Update local session error:', e);
      return false;
    } finally {
      this.isCreating = false;
    }
  }

  build() {
    Stack({ alignContent: Alignment.TopStart }) {
      Column() {
        Row() {
          Text('<')
            .fontSize(24)
            .fontColor('#333333')
            .padding({ right: 16 })
            .onClick(() => router.back())

          Text(this.isEditing ? '修改会话' : '新建会话')
            .fontSize(18)
            .fontWeight(FontWeight.Bold)
            .layoutWeight(1)
        }
        .width('100%')
        .height(56)
        .padding({ left: 16, right: 16 })
        .backgroundColor('#FFFFFF')

        Divider().height(0.5).color('#E0E0E0')

        Scroll() {
          Column() {
            Column() {
              Text('后端服务器')
                .fontSize(13)
                .fontColor('#999999')
                .margin({ bottom: 8 })
              Row() {
                Text(this.getSelectedBackend()?.notes || this.backendUrl || '选择后端')
                  .fontSize(15)
                  .fontColor(this.backendId ? '#333333' : '#AAAAAA')
                  .layoutWeight(1)
                  .maxLines(1)
                  .textOverflow({ overflow: TextOverflow.Ellipsis })
                if (!this.isEditing) {
                  Text(this.showBackendPicker ? '▲' : '▼')
                    .fontSize(12)
                    .fontColor('#999999')
                }
              }
              .width('100%')
              .height(44)
              .padding({ left: 12, right: 12 })
              .backgroundColor(this.isEditing ? '#EBEBEB' : '#F5F5F5')
              .borderRadius(8)
              .onClick(() => {
                if (!this.isEditing) {
                  this.showBackendPicker = !this.showBackendPicker;
                }
              })
            }
            .width('100%')
            .alignItems(HorizontalAlign.Start)
            .margin({ bottom: 20 })

            Column() {
              Text('工作目录')
                .fontSize(13)
                .fontColor('#999999')
                .margin({ bottom: 8 })
              if (this.isEditing) {
                // 编辑模式：只读显示
                Text(this.directory || '/')
                  .fontSize(15)
                  .fontColor('#666666')
                  .width('100%')
                  .height(44)
                  .padding({ left: 12, right: 12 })
                  .backgroundColor('#EBEBEB')
                  .borderRadius(8)
              } else {
                // 创建模式：可编辑
                TextInput({ placeholder: '输入目录路径，如 /home', text: this.directory })
                  .onChange(v => {
                    this.directory = v;
                    if (this.showPathPicker) {
                      this.schedulePathSearch(v);
                    }
                  })
                  .onFocus(() => {
                    this.showPathPicker = true;
                    this.schedulePathSearch(this.directory);
                  })
                  .onBlur(() => {
                    setTimeout(() => {
                      this.showPathPicker = false;
                    }, 200);
                  })
                  .height(44)
                  .backgroundColor('#F5F5F5')
                  .borderRadius(8)
              }
            }
            .width('100%')
            .alignItems(HorizontalAlign.Start)
            .margin({ bottom: 20 })

            Column() {
              Text('会话标题（可选）')
                .fontSize(13)
                .fontColor('#999999')
                .margin({ bottom: 8 })
              TextInput({ placeholder: '方便识别此会话', text: this.title })
                .onChange(v => this.title = v)
                .height(44)
                .backgroundColor('#F5F5F5')
                .borderRadius(8)
            }
            .width('100%')
            .alignItems(HorizontalAlign.Start)
            .margin({ bottom: 32 })

            if (this.errorMessage) {
              Text(this.errorMessage)
                .fontSize(13)
                .fontColor('#FF4D4F')
                .margin({ bottom: 16 })
            }
          }
          .width('100%')
          .padding({ left: 20, right: 20, top: 24 })
        }
        .layoutWeight(1)
        .align(Alignment.Top)

        Row({ space: 16 }) {
          Button('取消', { type: ButtonType.Normal })
            .layoutWeight(1)
            .height(44)
            .borderRadius(8)
            .backgroundColor('#F0F0F0')
            .fontColor('#333333')
            .onClick(() => router.back())

          Button(this.isCreating ? (this.isEditing ? '保存中...' : '创建中...') : (this.isEditing ? '保存' : '创建'), { type: ButtonType.Normal })
            .layoutWeight(1)
            .height(44)
            .borderRadius(8)
            .backgroundColor('#07C160')
            .fontColor('#FFFFFF')
            .enabled(!this.isCreating)
            .onClick(async () => {
              const success = await this.createSession();
              if (success) {
                AppStorage.SetOrCreate<string>('needRefreshSessions', 'true');
                router.back();
              }
            })
        }
        .width('100%')
        .padding({ left: 20, right: 20, bottom: 32 })
      }
      .width('100%')
      .height('100%')
      .backgroundColor('#FFFFFF')

      if (this.showPathPicker) {
        Column() {
          if (this.recentPaths.length > 0) {
            Text('最近使用')
              .fontSize(12)
              .fontColor('#999999')
              .fontWeight(FontWeight.Medium)
              .width('100%')
              .padding({ left: 12, right: 12, top: 10, bottom: 4 })
            ForEach(this.recentPaths.slice(0, 5), (path: string) => {
              Row() {
                Text(path)
                  .fontSize(13)
                  .fontColor('#333333')
                  .maxLines(1)
                  .textOverflow({ overflow: TextOverflow.Ellipsis })
                  .layoutWeight(1)
                Text('最近')
                  .fontSize(10)
                  .fontColor('#07C160')
                  .padding({ left: 6 })
              }
              .width('100%')
              .height(36)
              .padding({ left: 12, right: 12 })
              .onClick(() => {
                this.directory = path;
                this.showPathPicker = false;
                setTimeout(() => {
                  this.showPathPicker = true;
                  this.schedulePathSearch(path);
                }, 100);
              })
            })
            Divider().height(0.5).color('#EEEEEE').margin({ top: 4, bottom: 4 })
          }

          if (this.isSearchingPath) {
            Row() {
              LoadingProgress().width(14).height(14)
              Text('搜索中...').fontSize(12).fontColor('#999999').margin({ left: 6 })
            }
            .width('100%')
            .height(32)
            .padding({ left: 12 })
          } else if (this.pathSearchResults.length > 0) {
            ForEach(this.pathSearchResults.slice(0, 8), (item: FileNode) => {
              Row() {
                Text('📁')
                  .fontSize(12)
                  .margin({ right: 6 })
                Text(item.absolute)
                  .fontSize(13)
                  .fontColor('#333333')
                  .maxLines(1)
                  .textOverflow({ overflow: TextOverflow.Ellipsis })
                  .layoutWeight(1)
              }
              .width('100%')
              .height(36)
              .padding({ left: 12, right: 12 })
              .onClick(() => {
                this.directory = item.absolute;
                this.showPathPicker = false;
                // 核心修复：选中后立即触发下一级目录列表，而不是再次模糊搜索
                setTimeout(() => {
                  this.showPathPicker = true;
                  const nextQuery = this.directory.endsWith('/') ? this.directory : this.directory + '/';
                  this.listDirectory(nextQuery);
                }, 100);
              })
            })
          } else if (this.directory.length >= 1) {
            Text('未找到匹配目录')
              .fontSize(12)
              .fontColor('#999999')
              .width('100%')
              .padding({ left: 12, top: 8, bottom: 8 })
          }
        }
        .width('calc(100% - 40vp)')
        .constraintSize({ maxHeight: 300 })
        .backgroundColor('#FFFFFF')
        .borderRadius(8)
        .border({ width: 1, color: '#E0E0E0' })
        .shadow({ radius: 8, color: '#1A000000', offsetX: 0, offsetY: 2 })
        .margin({ top: 230, left: 20 })
        .zIndex(999)
      }

      if (this.showBackendPicker && !this.isEditing) {
        Column() {
          ForEach(this.backends, (backend: OpenCodeBackend) => {
            Row() {
              Text(backend.notes || backend.url)
                .fontSize(14)
                .fontColor('#333333')
                .layoutWeight(1)
                .maxLines(1)
                .textOverflow({ overflow: TextOverflow.Ellipsis })
              if (backend.id === this.backendId) {
                Text('✓').fontColor('#07C160').fontSize(14)
              }
            }
            .width('100%')
            .height(40)
            .padding({ left: 12, right: 12 })
            .onClick(() => this.selectBackend(backend))
          })
        }
        .width('calc(100% - 40vp)')
        .constraintSize({ maxHeight: 200 })
        .backgroundColor('#FFFFFF')
        .borderRadius(8)
        .border({ width: 1, color: '#E0E0E0' })
        .shadow({ radius: 8, color: '#1A000000', offsetX: 0, offsetY: 2 })
        .margin({ top: 145, left: 20 })
        .zIndex(999)
      }
    }
    .width('100%')
    .height('100%')
  }
}
