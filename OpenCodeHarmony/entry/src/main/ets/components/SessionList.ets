import { OpenCodeCore, OpenCodeBackend, OpenCodeSession } from '../core/OpenCodeCore';
import router from '@ohos.router';

interface SessionDisplayItem {
  id: string;
  name: string;
  backendUrl: string;
  backendId: string;
  directory: string;
  updatedAt: string;
  updatedAtTimestamp: number;
  isWorking: boolean;
  unreadCount: number;
  session: OpenCodeSession;
}

@Component
export struct SessionList {
  @State sessions: SessionDisplayItem[] = [];
  @State backends: OpenCodeBackend[] = [];
  @State selectedBackendId: string = '';
  @State isLoading: boolean = false;
  @StorageLink('refreshSessionsNow') @Watch('onRefreshNow') refreshSessionsNow: string = '';
  @StorageLink('themeAccent') themeAccent: string = '#07C160';
  @StorageLink('themeBgPrimary') themeBgPrimary: string = '#F5F5F5';
  @StorageLink('themeBgSecondary') themeBgSecondary: string = '#EDEDED';
  @StorageLink('themeBgCard') themeBgCard: string = '#FFFFFF';
  @StorageLink('themeTextPrimary') themeTextPrimary: string = '#333333';
  @StorageLink('themeTextSecondary') themeTextSecondary: string = '#888888';
  @StorageLink('themeTextTertiary') themeTextTertiary: string = '#999999';
  @StorageLink('themeTextMuted') themeTextMuted: string = '#BBBBBB';
  @StorageLink('themeBorderPrimary') themeBorderPrimary: string = '#E0E0E0';
  @StorageLink('themeStatusWorking') themeStatusWorking: string = '#2196F3';
  @StorageLink('themeStatusUnread') themeStatusUnread: string = '#FF3B30';
  @StorageLink('themeLink') themeLink: string = '#007DFF';
  @StorageLink('themeConfig') themeConfig: string = '#1677FF';
  private core: OpenCodeCore = OpenCodeCore.getInstance();

  aboutToAppear() {
    this.backends = this.core.getBackends();
    if (this.backends.length > 0) {
      this.selectedBackendId = this.backends[0].id;
    }
    this.refreshSessions();
  }

  aboutToDisappear() {
  }

  onRefreshNow() {
    if (this.refreshSessionsNow) {
      console.info('[SessionList] onRefreshNow triggered, value:', this.refreshSessionsNow);
      this.refreshSessions();
    }
  }

  async refreshSessions() {
    this.isLoading = true;
    try {
      this.backends = this.core.getBackends();
      if (!this.selectedBackendId && this.backends.length > 0) {
        this.selectedBackendId = this.backends[0].id;
      }
      const localSessions = this.core.getSessions();
      console.info('[SessionList] Loading local sessions:', localSessions.length);
      localSessions.forEach(s => {
        const p = this.core.getProjectById(s.id);
        console.info(`[SessionList] session=${s.id} isWorking=${p?.isWorking} unread=${p?.unreadCount}`);
      });
      
      this.sessions = localSessions.map((s): SessionDisplayItem => {
        const project = this.core.getProjectById(s.id);
        return {
          id: s.id,
          name: s.name,
          backendUrl: s.backendUrl,
          backendId: s.backendId,
          directory: s.directory,
          updatedAt: s.updatedAt,
          updatedAtTimestamp: s.updatedAtTimestamp,
          isWorking: project?.isWorking ?? false,
          unreadCount: project?.unreadCount ?? 0,
          session: {
            id: s.id,
            projectID: '',
            directory: s.directory,
            title: s.name,
            version: '1',
            time: { created: 0, updated: 0 }
          }
        };
      }).sort((a, b) => (b.updatedAtTimestamp ?? 0) - (a.updatedAtTimestamp ?? 0));
    } catch (e) {
      console.error('[SessionList] Failed to load sessions:', e);
      this.sessions = [];
    } finally {
      this.isLoading = false;
    }
  }

  build() {
    Column() {
      Row() {
        Text('会话')
          .fontSize(20)
          .fontWeight(FontWeight.Bold)
        Blank()
        Text('+')
          .fontSize(28)
          .fontColor(this.themeAccent)
          .onClick(() => {
            router.pushUrl({
              url: 'pages/SessionDetail',
              params: {
                backendId: this.selectedBackendId,
                backendUrl: this.backends.find(b => b.id === this.selectedBackendId)?.url || '',
                username: this.backends.find(b => b.id === this.selectedBackendId)?.username || '',
                authToken: this.backends.find(b => b.id === this.selectedBackendId)?.authToken || ''
              }
            });
          })
      }
      .width('100%')
      .padding({
        left: 20,
        right: 20,
        top: 15,
        bottom: 15
      })
      .backgroundColor(this.themeBgSecondary)

      if (this.backends.length === 0) {
        Column({ space: 16 }) {
          Stack() {
            Circle()
              .width(80)
              .height(80)
              .fill(this.themeBgCard)
            Text('💬')
              .fontSize(36)
          }
          .margin({ bottom: 8 })
          Text('暂无后端连接')
            .fontSize(18)
            .fontWeight(FontWeight.Medium)
            .fontColor(this.themeTextPrimary)
          Text('请先在"后端"页签添加 OpenCode 服务器')
            .fontSize(14)
            .fontColor(this.themeTextTertiary)
            .textAlign(TextAlign.Center)
        }
        .width('100%')
        .height('100%')
        .backgroundColor(this.themeBgCard)
        .justifyContent(FlexAlign.Center)
        .alignItems(HorizontalAlign.Center)
      } else {
        if (this.isLoading) {
          Column() {
            LoadingProgress()
              .width(48)
              .height(48)
            Text('加载中...')
              .fontSize(14)
              .fontColor(this.themeTextTertiary)
              .margin({ top: 12 })
          }
          .width('100%')
          .height('100%')
          .justifyContent(FlexAlign.Center)
          .alignItems(HorizontalAlign.Center)
        } else if (this.sessions.length === 0) {
          Column({ space: 16 }) {
            Stack() {
              Circle()
                .width(80)
                .height(80)
                .fill(this.themeBgCard)
              Text('💬')
                .fontSize(36)
            }
            .margin({ bottom: 8 })
            Text('暂无会话')
              .fontSize(18)
              .fontWeight(FontWeight.Medium)
              .fontColor(this.themeTextPrimary)
            Text('点击右上角 + 新建会话')
              .fontSize(14)
              .fontColor(this.themeTextTertiary)
            Button('新建会话', { type: ButtonType.Normal })
              .width(160)
              .height(44)
              .borderRadius(22)
              .backgroundColor(this.themeAccent)
              .fontColor('#FFFFFF')
              .fontSize(15)
              .margin({ top: 8 })
              .onClick(() => {
                router.pushUrl({
                  url: 'pages/SessionDetail',
                  params: {
                    backendId: this.selectedBackendId,
                    backendUrl: this.backends.find(b => b.id === this.selectedBackendId)?.url || '',
                    username: this.backends.find(b => b.id === this.selectedBackendId)?.username || '',
                    authToken: this.backends.find(b => b.id === this.selectedBackendId)?.authToken || ''
                  }
                });
              })
          }
          .width('100%')
          .height('100%')
          .backgroundColor(this.themeBgCard)
          .justifyContent(FlexAlign.Center)
          .alignItems(HorizontalAlign.Center)
        } else {
          List() {
            ForEach(this.sessions, (item: SessionDisplayItem) => {
              ListItem() {
                Row() {
                  Stack({ alignContent: Alignment.TopEnd }) {
                    Stack() {
                      Rect().fill(this.themeAccent).width(48).height(48).borderRadius(8)
                      Text(item.name.charAt(0).toUpperCase())
                        .fontSize(20).fontColor('#FFFFFF').fontWeight(FontWeight.Bold)
                    }

                    if (item.isWorking) {
                      Circle().width(12).height(12)
                        .fill(this.themeStatusWorking)
                        .stroke('#FFFFFF')
                        .strokeWidth(2)
                        .offset({ x: 4, y: -4 })
                    } else if (item.unreadCount > 0) {
                      Circle().width(12).height(12)
                        .fill(this.themeStatusUnread)
                        .stroke('#FFFFFF')
                        .strokeWidth(2)
                        .offset({ x: 4, y: -4 })
                    }
                  }
                  .width(48).height(48)
                  .margin({ right: 16 })

                  Column() {
                    Text(item.name)
                      .fontSize(16)
                      .fontWeight(FontWeight.Medium)
                      .maxLines(1)
                    Row() {
                      Text('后端: ' + (this.backends.find(b => b.id === item.backendId)?.notes || item.backendUrl))
                        .fontSize(11)
                        .fontColor(this.themeAccent)
                        .maxLines(1)
                        .textOverflow({ overflow: TextOverflow.Ellipsis })
                      Text(' | ')
                        .fontSize(11)
                        .fontColor('#CCCCCC')
                      Text(item.directory || '/')
                        .fontSize(11)
                        .fontColor(this.themeTextSecondary)
                        .maxLines(1)
                        .textOverflow({ overflow: TextOverflow.Ellipsis })
                        .layoutWeight(1)
                      // hasPendingWork = isWorking || unreadCount > 0（未读数标记）
                      if (item.unreadCount > 0) {
                        Text(`${item.unreadCount}`)
                          .fontSize(10)
                          .fontColor('#FFFFFF')
                          .backgroundColor(this.themeStatusUnread)
                          .borderRadius(8)
                          .padding({ left: 6, right: 6, top: 2, bottom: 2 })
                          .margin({ left: 4 })
                      }
                    }
                    .margin({ top: 2 })
                    Text(item.updatedAt)
                      .fontSize(11)
                      .fontColor(this.themeTextMuted)
                      .margin({ top: 2 })
                  }
                  .layoutWeight(1)
                  .alignItems(HorizontalAlign.Start)

                  Text('配置')
                      .fontSize(13)
                      .fontColor(this.themeConfig)
                      .padding(8)
                      .onClick(() => {
                        const backend = this.backends.find(b => b.id === item.backendId);
                        router.pushUrl({
                          url: 'pages/SessionDetail',
                          params: {
                            backendId: item.backendId,
                            backendUrl: item.backendUrl,
                            username: backend?.username || '',
                            authToken: backend?.authToken || '',
                            title: item.name,
                            directory: item.directory,
                            isEditing: true,
                            id: item.id
                          }
                        });
                      })
                }
                .width('100%')
                .padding({
                  left: 16,
                  right: 16,
                  top: 12,
                  bottom: 12
                })
                .backgroundColor(this.themeBgCard)
              }
              .onClick(() => {
                this.core.setCurrentProject(item.id);
                this.core.setCurrentSession(item.id);
                router.pushUrl({
                  url: 'pages/ChatPage',
                  params: {
                    sessionId: item.id,
                    sessionName: item.name,
                    backendUrl: item.backendUrl,
                    username: this.backends.find(b => b.id === item.backendId)?.username || '',
                    authToken: this.backends.find(b => b.id === item.backendId)?.authToken || '',
                    directory: item.directory
                  }
                });
              })
            }, (item: SessionDisplayItem) => item.id)
          }
          .divider({ strokeWidth: 0.5, color: this.themeBorderPrimary, startMargin: 76 })
          .backgroundColor(this.themeBgCard)
          .flexGrow(1)
        }
      }
    }
    .width('100%')
    .backgroundColor(this.themeBgSecondary)
  }
}
