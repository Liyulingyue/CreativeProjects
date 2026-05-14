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
    if (!query) {
      this.listDirectory('/');
      return;
    }
    if (query.startsWith('/') || /^[a-zA-Z]:/.test(query)) {
      this.listDirectory(query);
      return;
    }
    this.pathSearchTimer = setTimeout(() => {
      this.searchPaths(query);
    }, 300) as number;
  }

  private searchPaths(query: string) {
    if (!this.backendUrl || this.backendUrl === 'http://') {
      this.isSearchingPath = false;
      return;
    }
    this.isSearchingPath = true;
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = this.backendUrl.replace(/\/+$/, '') + '/find/file';
    const params = `query=${encodeURIComponent(query)}&type=directory&limit=20`;
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
                this.pathSearchResults = parsed.map((p: string): FileNode => ({
                  name: p.split('/').pop() || p,
                  path: p,
                  absolute: p.startsWith('/') ? p : '/' + p,
                  type: 'directory',
                  ignored: false
                }));
              }
            } catch {}
          }
        }
      );
    } catch {}
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
    const params = `path=${encodeURIComponent(path)}`;
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
                this.pathSearchResults = parsed.filter((item: FileNode) => item.type === 'directory' && !item.ignored);
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
    this.cancelRequest();
    this.currentRequest = http.createHttp();

    const url = this.backendUrl.replace(/\/+$/, '') + '/session';
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'x-opencode-directory': encodeURIComponent(this.directory || '/')
    };
    if (this.authToken) {
      headers['Authorization'] = 'Basic ' + this.base64Encode('opencode:' + this.authToken);
    }

    const body: Record<string, string> = {};
    if (this.title) {
      body['title'] = this.title;
    }

    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.POST,
            header: headers,
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
        const session = JSON.parse(result.result as string) as SessionResponse;
        console.info('[SessionDetail] Session created:', session.id);
        if (this.directory) {
          this.addRecentPath(this.directory);
        }
        return true;
      } else {
        this.errorMessage = `创建失败: HTTP ${result.responseCode}`;
        return false;
      }
    } catch (e) {
      this.errorMessage = `创建失败: ${e}`;
      console.error('[SessionDetail] Create session error:', e);
      return false;
    } finally {
      this.isCreating = false;
      this.cancelRequest();
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

          Text('新建会话')
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
                Text(this.showBackendPicker ? '▲' : '▼')
                  .fontSize(12)
                  .fontColor('#999999')
              }
              .width('100%')
              .height(44)
              .padding({ left: 12, right: 12 })
              .backgroundColor('#F5F5F5')
              .borderRadius(8)
              .onClick(() => this.showBackendPicker = !this.showBackendPicker)
            }
            .width('100%')
            .alignItems(HorizontalAlign.Start)
            .margin({ bottom: 20 })

            Column() {
              Text('工作目录')
                .fontSize(13)
                .fontColor('#999999')
                .margin({ bottom: 8 })
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

          Button(this.isCreating ? '创建中...' : '创建', { type: ButtonType.Normal })
            .layoutWeight(1)
            .height(44)
            .borderRadius(8)
            .backgroundColor('#07C160')
            .fontColor('#FFFFFF')
            .enabled(!this.isCreating)
            .onClick(async () => {
              const success = await this.createSession();
              if (success) {
                AppStorage.SetOrCreate<number>('sessionRefreshTag', Date.now());
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
                Text(item.path)
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
                this.directory = item.path;
                this.showPathPicker = false;
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
        .margin({ top: 200, left: 20 })
        .zIndex(999)
      }

      if (this.showBackendPicker) {
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
        .margin({ top: 140, left: 20 })
        .zIndex(999)
      }
    }
    .width('100%')
    .height('100%')
  }
}
