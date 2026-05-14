import { OpenCodeCore, OpenCodeBackend } from '../core/OpenCodeCore';
import router from '@ohos.router';
import http from '@ohos.net.http';

@Entry
@Component
struct SessionDetail {
  @State selectedBackendId: string = '';
  @State worktree: string = '';
  @State notes: string = '';
  @State isEdit: boolean = false;
  @State editId: string = '';
  @State backends: OpenCodeBackend[] = [];
  @State recentPaths: string[] = [];
  @State showBackendPicker: boolean = false;
  @State showPathPicker: boolean = false;
  @State pathSearchResults: string[] = [];
  @State isSearchingPath: boolean = false;
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  private pathSearchTimer: number = -1;
  private currentRequest: http.HttpRequest | null = null;

  aboutToAppear() {
    this.backends = this.core.getBackends();
    this.recentPaths = AppStorage.get<string[]>('recentWorktreePaths') ?? [];
    const params = router.getParams() as Record<string, Object>;
    if (params && params['id']) {
      this.isEdit = true;
      this.editId = params['id'] as string;
      const project = this.core.getProjectById(this.editId);
      if (project) {
        this.notes = project.notes ?? '';
        this.worktree = project.path;
        const matched = this.backends.find(b => b.id === project.backendId);
        if (matched) {
          this.selectedBackendId = matched.id;
        }
      }
    }
    if (this.backends.length > 0 && !this.selectedBackendId) {
      this.selectedBackendId = this.backends[0].id;
    }
  }

  private getSelectedBackend(): OpenCodeBackend | undefined {
    return this.backends.find(b => b.id === this.selectedBackendId);
  }

  private getActiveUrl(): string {
    return this.getSelectedBackend()?.url ?? '';
  }

  private getActiveToken(): string {
    return this.getSelectedBackend()?.authToken ?? '';
  }

  private addRecentPath(path: string) {
    const normalized = path.replace(/\/+$/, '');
    const existing = this.recentPaths.filter(p => p !== normalized);
    const updated = [normalized, ...existing].slice(0, 10);
    this.recentPaths = updated;
    AppStorage.SetOrCreate<string[]>('recentWorktreePaths', updated);
  }

  private schedulePathSearch(query: string) {
    if (this.pathSearchTimer !== -1) {
      clearTimeout(this.pathSearchTimer);
    }
    if (!query || query.length < 2) {
      this.listDirectory('.');
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
    this.isSearchingPath = true;
    const url = this.getActiveUrl().replace(/\/+$/, '');
    if (!url || url === 'http://') {
      this.isSearchingPath = false;
      return;
    }
    if (this.currentRequest) {
      this.currentRequest.destroy();
      this.currentRequest = null;
    }
    const token = this.getActiveToken();
    this.currentRequest = http.createHttp();
    const headers: Record<string, string> = {};
    if (token) {
      headers['authorization'] = 'Basic ' + this.base64Encode('opencode:' + token);
    }
    const searchUrl = url + '/find/file?query=' + encodeURIComponent(query) + '&type=directory&limit=20';
    this.currentRequest.request(
      searchUrl,
      {
        method: http.RequestMethod.GET,
        header: headers,
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
              this.pathSearchResults = parsed.filter((s: string) => !!s);
            }
          } catch {}
        }
      }
    );
  }

  private listDirectory(path: string) {
    this.isSearchingPath = true;
    const url = this.getActiveUrl().replace(/\/+$/, '');
    if (!url || url === 'http://') {
      this.isSearchingPath = false;
      return;
    }
    if (this.currentRequest) {
      this.currentRequest.destroy();
      this.currentRequest = null;
    }
    const token = this.getActiveToken();
    this.currentRequest = http.createHttp();
    const headers: Record<string, string> = {};
    if (token) {
      headers['authorization'] = 'Basic ' + this.base64Encode('opencode:' + token);
    }
    const listUrl = url + '/file?path=' + encodeURIComponent(path || '.');
    this.currentRequest.request(
      listUrl,
      {
        method: http.RequestMethod.GET,
        header: headers,
        connectTimeout: 5000,
        readTimeout: 5000,
      },
      (err, data) => {
        this.isSearchingPath = false;
        this.currentRequest = null;
        if (!err && data.responseCode === 200) {
          try {
            const parsed = JSON.parse(data.result as string) as Record<string, string>[];
            if (Array.isArray(parsed)) {
              this.pathSearchResults = parsed
                .filter((item: Record<string, string>) => item.type === 'directory')
                .map((item: Record<string, string>) => item.path || item.name)
                .filter((s: string) => !!s);
            }
          } catch {}
        }
      }
    );
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

  private saveSession() {
    const url = this.getActiveUrl().replace(/\/+$/, '');
    const token = this.getActiveToken();
    if (!url || url === 'http://' || !this.worktree) {
      return;
    }
    this.addRecentPath(this.worktree);
    if (this.isEdit) {
      this.core.updateProject(this.editId, this.notes, url, token, this.worktree, this.notes);
    } else {
      this.core.addProjectWithBackend(this.notes, url, token, this.worktree, this.notes);
    }
    AppStorage.SetOrCreate<number>('sessionRefreshTag', Date.now());
    router.back();
  }

  @Builder
  BackendPickerDropdown() {
    Column() {
      ForEach(this.backends, (backend: OpenCodeBackend) => {
        Row() {
          Text(backend.notes || backend.url)
            .fontSize(14)
            .fontColor('#333333')
            .layoutWeight(1)
            .maxLines(1)
            .textOverflow({ overflow: TextOverflow.Ellipsis })
          if (backend.id === this.selectedBackendId) {
            Text('✓').fontColor('#07C160').fontSize(14)
          }
        }
        .width('100%')
        .height(40)
        .padding({ left: 12, right: 12 })
        .onClick(() => {
          this.selectedBackendId = backend.id;
          this.showBackendPicker = false;
        })
      })
      if (this.backends.length === 0) {
        Text('暂无后端，请先添加')
          .fontSize(13)
          .fontColor('#999999')
          .width('100%')
          .padding({ left: 12, top: 12, bottom: 12 })
      }
    }
    .width('100%')
    .backgroundColor('#FFFFFF')
    .borderRadius(8)
    .border({ width: 1, color: '#E0E0E0' })
  }

  build() {
    Stack({ alignContent: Alignment.TopStart }) {
      Column() {
        Row() {
          Text('←')
            .fontSize(24)
            .fontColor('#333333')
            .padding({ right: 16 })
            .onClick(() => router.back())

          Text(this.isEdit ? '编辑会话' : '新建会话')
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
              Text('后端连接')
                .fontSize(13)
                .fontColor('#999999')
                .margin({ bottom: 8 })
              Row() {
                Text(this.getSelectedBackend()?.notes || this.getSelectedBackend()?.url || '选择后端')
                  .fontSize(15)
                  .fontColor(this.selectedBackendId ? '#333333' : '#AAAAAA')
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
              Text('工作文件夹目录')
                .fontSize(13)
                .fontColor('#999999')
                .margin({ bottom: 8 })
              TextInput({ placeholder: '输入搜索目录', text: this.worktree })
                  .onChange(v => {
                    this.worktree = v;
                    if (this.showPathPicker) {
                      if (v && v.length >= 1) {
                        this.schedulePathSearch(v);
                      } else {
                        this.listDirectory('.');
                      }
                    }
                  })
                .onFocus(() => {
                  this.showPathPicker = true;
                  if (this.worktree && this.worktree.length >= 1) {
                    this.schedulePathSearch(this.worktree);
                  } else {
                    this.listDirectory('.');
                  }
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

            // 路径选择下拉框（绝对定位浮层）
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
                      this.worktree = path;
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
                  ForEach(this.pathSearchResults.slice(0, 8), (path: string) => {
                    Row() {
                      Text(path)
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
                      this.worktree = path;
                      this.showPathPicker = false;
                    })
                  })
                } else if (this.worktree.length >= 2) {
                  Text('未找到匹配目录')
                    .fontSize(12)
                    .fontColor('#999999')
                    .width('100%')
                    .padding({ left: 12, top: 8, bottom: 8 })
                }
              }
              .width('calc(100% - 40vp)')
              .backgroundColor('#FFFFFF')
              .borderRadius(8)
              .border({ width: 1, color: '#E0E0E0' })
              .shadow({ radius: 8, color: '#1A000000', offsetX: 0, offsetY: 2 })
              .margin({ top: 220, left: 20 })
              .zIndex(999)
            }

            Column() {
              Text('备注')
                .fontSize(13)
                .fontColor('#999999')
                .margin({ bottom: 8 })
              TextInput({ placeholder: '方便识别此会话', text: this.notes })
                .onChange(v => this.notes = v)
                .height(44)
                .backgroundColor('#F5F5F5')
                .borderRadius(8)
            }
            .width('100%')
            .alignItems(HorizontalAlign.Start)
            .margin({ bottom: 32 })
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

          Button('确认', { type: ButtonType.Normal })
            .layoutWeight(1)
            .height(44)
            .borderRadius(8)
            .backgroundColor('#07C160')
            .fontColor('#FFFFFF')
            .enabled(this.worktree.length > 0 && this.selectedBackendId.length > 0)
            .onClick(() => this.saveSession())
        }
        .width('100%')
        .padding({ left: 20, right: 20, bottom: 32 })
      }
      .width('100%')
      .height('100%')
      .backgroundColor('#FFFFFF')

      // 后端选择下拉框（绝对定位，不影响布局）
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
              if (backend.id === this.selectedBackendId) {
                Text('✓').fontColor('#07C160').fontSize(14)
              }
            }
            .width('100%')
            .height(40)
            .padding({ left: 12, right: 12 })
            .onClick(() => {
              this.selectedBackendId = backend.id;
              this.showBackendPicker = false;
            })
          })
          if (this.backends.length === 0) {
            Text('暂无后端，请先添加')
              .fontSize(13)
              .fontColor('#999999')
              .width('100%')
              .padding({ left: 12, top: 12, bottom: 12 })
          }
        }
        .width('calc(100% - 40vp)')
        .backgroundColor('#FFFFFF')
        .borderRadius(8)
        .border({ width: 1, color: '#E0E0E0' })
        .shadow({ radius: 8, color: '#1A000000', offsetX: 0, offsetY: 2 })
        .margin({ top: 128, left: 20 })
        .zIndex(999)
      }
    }
    .width('100%')
    .height('100%')
  }
}
