import { OpenCodeCore, OpenCodeBackend, OpenCodeSession, OpenCodeProviderModel } from '../core/OpenCodeCore';
import { SessionDetailViewModel, FileNode } from '../viewmodel/SessionDetailViewModel';
import { RemoteSessionPicker } from '../components/RemoteSessionPicker';
import { PathAutocomplete } from '../components/PathAutocomplete';
import { BackendPicker } from '../components/BackendPicker';
import router from '@ohos.router';
import http from '@ohos.net.http';

@Entry
@Component
struct SessionDetail {
  @State title: string = '';
  @State directory: string = '/';
  @State isCreating: boolean = false;
  @State errorMessage: string = '';
  @State backendId: string = '';
  @State backendUrl: string = '';
  @State username: string = '';
  @State authToken: string = '';
  @State backends: OpenCodeBackend[] = [];
  @State showBackendPicker: boolean = false;
  @State recentPaths: string[] = [];
  @State showPathPicker: boolean = false;
  @State pathSearchResults: FileNode[] = [];
  @State isSearchingPath: boolean = false;
  @State isEditing: boolean = false;
  @State editingId: string = '';
  @State remoteSessions: OpenCodeSession[] = [];
  @State isLoadingRemoteSessions: boolean = false;
  @State showRemoteSessionPicker: boolean = false;
  @State selectedRemoteSessionId: string = '';
  @State availableModels: OpenCodeProviderModel[] = [];
  @State isLoadingModels: boolean = false;
  @State selectedModel: string = '';
  @State showModelPicker: boolean = false;

  private viewModel: SessionDetailViewModel = new SessionDetailViewModel();
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  private currentRequest: http.HttpRequest | null = null;
  private pathSearchTimer: number = -1;

  aboutToAppear() {
    this.backends = this.core.getBackends();
    const params = router.getParams() as Record<string, Object>;
    if (params) {
      this.backendId = (params['backendId'] as string) || '';
      this.backendUrl = (params['backendUrl'] as string) || '';
      this.username = (params['username'] as string) || '';
      this.authToken = (params['authToken'] as string) || '';
      this.isEditing = (params['isEditing'] as boolean) || false;
      this.editingId = (params['id'] as string) || '';
      if (this.isEditing) {
        this.title = (params['title'] as string) || '';
        this.directory = (params['directory'] as string) || '/';
        const project = this.core.getProjects().find(p => p.id === this.editingId);
        if (project) {
          if (project.remoteSessionId) {
            this.selectedRemoteSessionId = project.remoteSessionId;
          }
          if (project.preferredModel) {
            this.selectedModel = project.preferredModel;
          }
          console.info('[SessionDetail] Loaded project model:', this.selectedModel);
        }
      }
    }
    if (!this.backendId && this.backends.length > 0) {
      this.selectBackend(this.backends[0]);
    } else if (this.backendId) {
      this.fetchRemoteSessions();
      this.fetchModels();
    }
    this.recentPaths = AppStorage.get<string[]>('recentWorktreePaths') ?? [];
  }

  private async fetchRemoteSessions() {
    if (!this.backendUrl) return;
    this.isLoadingRemoteSessions = true;
    try {
        this.remoteSessions = await this.viewModel.fetchRemoteSessions(this.backendUrl, this.username, this.authToken, this.directory);
    } catch (e) {
      console.error('[SessionDetail] Fetch remote sessions error:', e);
    } finally {
      this.isLoadingRemoteSessions = false;
    }
  }

  private async fetchModels() {
    if (!this.backendUrl) return;
    this.isLoadingModels = true;
    try {
        this.availableModels = await this.viewModel.fetchModels(this.backendUrl, this.username, this.authToken);
      // 兼容旧数据：历史上可能只保存了 model.id，这里自动迁移为 provider/model。
      if (this.selectedModel && !this.selectedModel.includes('/')) {
        const matches = this.availableModels.filter(
          m => (m.id === this.selectedModel || m.modelID === this.selectedModel) && !!m.providerID
        );
        if (matches.length === 1) {
          this.selectedModel = this.getModelValue(matches[0]);
        }
      }
    } catch (e) {
      console.error('[SessionDetail] Fetch models error:', e);
    } finally {
      this.isLoadingModels = false;
    }
  }

  private getModelValue(model: OpenCodeProviderModel): string {
    const stableModelId = model.modelID || model.id;
    if (model.providerID) {
      return `${model.providerID}/${stableModelId}`;
    }
    return stableModelId;
  }

  private selectBackend(backend: OpenCodeBackend) {
    this.backendId = backend.id;
    this.backendUrl = backend.url;
    this.username = backend.username;
    this.authToken = backend.authToken;
    this.showBackendPicker = false;
    this.fetchRemoteSessions();
    this.fetchModels();
  }

  private getSelectedBackend(): OpenCodeBackend | undefined {
    return this.backends.find(b => b.id === this.backendId);
  }

  private onRecentPathSelect(path: string) {
    this.directory = path;
    this.showPathPicker = false;
    setTimeout(() => {
      this.showPathPicker = true;
      this.schedulePathSearch(path);
    }, 100);
  }

  private onPathSelect(path: string) {
    this.directory = path;
    this.showPathPicker = false;
    setTimeout(() => {
      this.showPathPicker = true;
      const nextQuery = this.directory.endsWith('/') ? this.directory : this.directory + '/';
      this.listDirectory(nextQuery);
    }, 100);
  }

  private schedulePathSearch(query: string) {
    this.viewModel.schedulePathSearch(
      query,
      (path) => this.listDirectory(path),
      (searchQuery, fullPath) => this.searchPaths(searchQuery, fullPath),
      this.pathSearchTimer,
      (timer) => this.pathSearchTimer = timer
    );
  }

  private searchPaths(query: string, fullPath: string) {
    this.viewModel.searchPaths(
      this.backendUrl,
      this.username,
      this.authToken,
      this.directory,
      query,
      fullPath,
      (results) => this.pathSearchResults = results,
      (isSearching) => this.isSearchingPath = isSearching,
      this.currentRequest,
      (req) => this.currentRequest = req
    );
  }

  private listDirectory(path: string) {
    this.viewModel.listDirectory(
      this.backendUrl,
      this.username,
      this.authToken,
      this.directory,
      path,
      (results) => this.pathSearchResults = results,
      (isSearching) => this.isSearchingPath = isSearching,
      this.currentRequest,
      (req) => this.currentRequest = req
    );
  }

  private onRemoteSessionSelect(session: OpenCodeSession | null) {
    this.showRemoteSessionPicker = false;
    if (session) {
      this.selectedRemoteSessionId = session.id;
      if (session.directory) {
        this.directory = session.directory;
      }
      // 绑定远程会话时，标题默认为远程会话名称
      if (session.title) {
        this.title = session.title;
      }
    } else {
      this.selectedRemoteSessionId = '';
    }
  }

  private async createSession(): Promise<boolean> {
    if (!this.backendUrl || this.isCreating) return false;

    this.isCreating = true;
    this.errorMessage = '';
    console.info('[SessionDetail] >>> createSession, selectedModel:', this.selectedModel);

    try {
      const success = await this.viewModel.createOrUpdateSession(
        this.isEditing,
        this.editingId,
        this.title,
        this.directory,
        this.backendUrl,
        this.username,
        this.authToken,
        this.backendId,
        this.selectedRemoteSessionId,
        this.selectedModel
      );

      if (success && this.directory) {
        this.recentPaths = this.viewModel.addRecentPath(this.directory, this.recentPaths);
      }

      return success;
    } catch (e) {
      this.errorMessage = `${this.isEditing ? '更新' : '创建'}失败: ${e}`;
      console.error('[SessionDetail] Create/Update local session error:', e);
      return false;
    } finally {
      this.isCreating = false;
    }
  }

  @Builder
  buildBackendSection() {
    Column() {
      Text('模型 / 后端服务器')
        .fontSize(13)
        .fontColor('#999999')
        .margin({ bottom: 8 })
      Row() {
        Text(this.getSelectedBackend()?.notes || this.backendUrl || '选择模型')
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
  }

  @Builder
  buildDirectorySection() {
    Column() {
      Text('工作目录')
        .fontSize(13)
        .fontColor('#999999')
        .margin({ bottom: 8 })
      if (this.isEditing) {
        Text(this.directory || '/')
          .fontSize(15)
          .fontColor('#666666')
          .width('100%')
          .height(44)
          .padding({ left: 12, right: 12 })
          .backgroundColor('#EBEBEB')
          .borderRadius(8)
      } else if (this.selectedRemoteSessionId) {
        Row() {
          Image($r('app.media.startIcon')).width(16).height(16).margin({ right: 8 }).opacity(0.6)
          Text(this.directory || '/')
            .fontSize(15)
            .fontColor('#666666')
            .layoutWeight(1)
          Text('继承自 Session')
            .fontSize(12)
            .fontColor('#07C160')
        }
        .width('100%')
        .height(44)
        .padding({ left: 12, right: 12 })
        .backgroundColor('#F0F9EB')
        .borderRadius(8)
      } else {
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
  }

  @Builder
  buildModelSection() {
    Column() {
      Text('接下来对话使用的模型 (可选)')
        .fontSize(13)
        .fontColor('#999999')
        .margin({ bottom: 8 })

      Row() {
        if (this.isLoadingModels) {
          LoadingProgress().width(20).height(20).margin({ right: 8 })
        }
        Text(this.selectedModel || '使用服务器默认模型')
          .fontSize(15)
          .fontColor(this.selectedModel ? '#333333' : '#AAAAAA')
          .layoutWeight(1)
          .maxLines(1)
          .textOverflow({ overflow: TextOverflow.Ellipsis })

        Text(this.showModelPicker ? '▲' : '▼')
          .fontSize(12)
          .fontColor('#999999')
      }
      .width('100%')
      .height(44)
      .padding({ left: 12, right: 12 })
      .backgroundColor('#F5F5F5')
      .borderRadius(8)
      .onClick(() => {
        this.showModelPicker = !this.showModelPicker;
        if (this.showModelPicker && this.availableModels.length === 0) {
          this.fetchModels();
        }
      })

      if (this.showModelPicker) {
        Scroll() {
          Column() {
            // Option to clear selection
            Row() {
              Text('使用服务器默认模型')
                .fontSize(14)
                .fontColor(this.selectedModel === '' ? '#07C160' : '#666666')
                .fontWeight(this.selectedModel === '' ? FontWeight.Bold : FontWeight.Normal)
              if (this.selectedModel === '') {
                Image($r('app.media.startIcon')).width(16).height(16).fillColor('#07C160')
              }
            }
            .width('100%')
            .padding(12)
            .justifyContent(FlexAlign.SpaceBetween)
            .onClick(() => {
              this.selectedModel = '';
              this.showModelPicker = false;
            })

            Divider().color('#F0F0F0').margin({ left: 12, right: 12 })

            ForEach(this.availableModels, (model: OpenCodeProviderModel) => {
              Row() {
                Column() {
                  Text(`${model.providerName || 'Unknown'} · ${model.name || model.id}`)
                    .fontSize(14)
                    .fontColor(this.selectedModel === this.getModelValue(model) ? '#07C160' : '#333333')
                    .fontWeight(this.selectedModel === this.getModelValue(model) ? FontWeight.Bold : FontWeight.Normal)
                }
                .alignItems(HorizontalAlign.Start)

                if (this.selectedModel === this.getModelValue(model)) {
                  Image($r('app.media.startIcon')).width(16).height(16).fillColor('#07C160')
                }
              }
              .width('100%')
              .padding(12)
              .justifyContent(FlexAlign.SpaceBetween)
              .onClick(() => {
                this.selectedModel = this.getModelValue(model);
                console.info('[SessionDetail] Model selected:', this.selectedModel);
                this.showModelPicker = false;
              })

              Divider().color('#F0F0F0').margin({ left: 12, right: 12 })
            }, (model: OpenCodeProviderModel) => this.getModelValue(model))

            if (!this.isLoadingModels && this.availableModels.length === 0) {
              Text('未获取到可用模型')
                .fontSize(13)
                .fontColor('#999999')
                .margin(20)
            }
          }
          .backgroundColor('#F9F9F9')
          .borderRadius(8)
          .margin({ top: 4 })
        }
        .width('100%')
        .constraintSize({ maxHeight: 250 })
      }
    }
    .width('100%')
    .alignItems(HorizontalAlign.Start)
    .margin({ bottom: 20 })
  }

  @Builder
  buildRemoteSessionSection() {
    Column() {
      Text('绑定远程会话 (可选)')
        .fontSize(13)
        .fontColor('#999999')
        .margin({ bottom: 8 })

      Row() {
        if (this.isLoadingRemoteSessions) {
          LoadingProgress().width(20).height(20).margin({ right: 8 })
        }
        Text(this.selectedRemoteSessionId ?
          (this.remoteSessions.find(s => s.id === this.selectedRemoteSessionId)?.title || this.selectedRemoteSessionId) :
          '不绑定 (创建新会话)')
          .fontSize(15)
          .fontColor(this.selectedRemoteSessionId ? '#333333' : '#AAAAAA')
          .layoutWeight(1)
          .maxLines(1)
          .textOverflow({ overflow: TextOverflow.Ellipsis })

        if (!this.isEditing) {
          Text(this.showRemoteSessionPicker ? '▲' : '▼')
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
          this.showRemoteSessionPicker = !this.showRemoteSessionPicker;
          if (this.showRemoteSessionPicker && this.remoteSessions.length === 0) {
            this.fetchRemoteSessions();
          }
        }
      })

      if (this.showRemoteSessionPicker && !this.isEditing) {
        RemoteSessionPicker({
          sessions: this.remoteSessions,
          selectedId: this.selectedRemoteSessionId,
          isLoading: this.isLoadingRemoteSessions,
          showPicker: $showRemoteSessionPicker,
          onSelect: (session: OpenCodeSession | null): void => this.onRemoteSessionSelect(session)
        })
      }
    }
    .width('100%')
    .alignItems(HorizontalAlign.Start)
    .margin({ bottom: 20 })
  }

  @Builder
  buildTitleSection() {
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
            this.buildBackendSection()
            this.buildDirectorySection()
            this.buildRemoteSessionSection()
            this.buildTitleSection()
            this.buildModelSection()

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
        PathAutocomplete({
          recentPaths: this.recentPaths,
          searchResults: this.pathSearchResults,
          isSearching: this.isSearchingPath,
          showPicker: $showPathPicker,
          onPathSelect: (path: string): void => this.onPathSelect(path),
          onRecentSelect: (path: string): void => this.onRecentPathSelect(path)
        })
      }

      if (this.showBackendPicker && !this.isEditing) {
        BackendPicker({
          backends: this.backends,
          selectedId: this.backendId,
          onSelect: (backend: OpenCodeBackend): void => this.selectBackend(backend)
        })
      }
    }
    .width('100%')
    .height('100%')
  }
}
