import { OpenCodeCore, OpenCodeBackend, OpenCodeSession } from '../core/OpenCodeCore';
import router from '@ohos.router';

interface SessionDisplayItem {
  id: string;
  name: string;
  backendUrl: string;
  backendId: string;
  directory: string;
  updatedAt: string;
  session: OpenCodeSession;
}

@Component
export struct SessionList {
  @State sessions: SessionDisplayItem[] = [];
  @State backends: OpenCodeBackend[] = [];
  @State selectedBackendId: string = '';
  @State isLoading: boolean = false;
  @StorageLink('needRefreshSessions') @Watch('onNeedRefreshChanged') needRefreshSessions: string = 'false';
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

  onNeedRefreshChanged() {
    if (this.needRefreshSessions === 'true') {
      this.needRefreshSessions = 'false';
      this.refreshSessions();
    }
  }

  async refreshSessions() {
    this.isLoading = true;
    try {
      const localSessions = this.core.getSessions();
      console.info('[SessionList] Loading local sessions:', localSessions.length);
      
      this.sessions = localSessions.map((s): SessionDisplayItem => ({
        id: s.id,
        name: s.name,
        backendUrl: s.backendUrl,
        backendId: s.backendId,
        directory: s.directory,
        updatedAt: s.updatedAt,
        session: {
          id: s.id,
          projectID: '',
          directory: s.directory,
          title: s.name,
          version: '1',
          time: { created: 0, updated: 0 }
        }
      }));
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
          .fontColor('#07C160')
          .onClick(() => {
            router.pushUrl({
              url: 'pages/SessionDetail',
              params: {
                backendId: this.selectedBackendId,
                backendUrl: this.backends.find(b => b.id === this.selectedBackendId)?.url || '',
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
      .backgroundColor('#EDEDED')

      if (this.backends.length === 0) {
        Column({ space: 16 }) {
          Stack() {
            Circle()
              .width(80)
              .height(80)
              .fill('#F0F5FF')
            Text('💬')
              .fontSize(36)
          }
          .margin({ bottom: 8 })
          Text('暂无后端连接')
            .fontSize(18)
            .fontWeight(FontWeight.Medium)
            .fontColor('#333333')
          Text('请先在"后端"页签添加 OpenCode 服务器')
            .fontSize(14)
            .fontColor('#999999')
            .textAlign(TextAlign.Center)
        }
        .width('100%')
        .height('100%')
        .backgroundColor('#FFFFFF')
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
              .fontColor('#999999')
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
                .fill('#F0F5FF')
              Text('💬')
                .fontSize(36)
            }
            .margin({ bottom: 8 })
            Text('暂无会话')
              .fontSize(18)
              .fontWeight(FontWeight.Medium)
              .fontColor('#333333')
            Text('点击右上角 + 新建会话')
              .fontSize(14)
              .fontColor('#999999')
            Button('新建会话', { type: ButtonType.Normal })
              .width(160)
              .height(44)
              .borderRadius(22)
              .backgroundColor('#07C160')
              .fontColor('#FFFFFF')
              .fontSize(15)
              .margin({ top: 8 })
              .onClick(() => {
                router.pushUrl({
                  url: 'pages/SessionDetail',
                  params: {
                    backendId: this.selectedBackendId,
                    backendUrl: this.backends.find(b => b.id === this.selectedBackendId)?.url || '',
                    authToken: this.backends.find(b => b.id === this.selectedBackendId)?.authToken || ''
                  }
                });
              })
          }
          .width('100%')
          .height('100%')
          .backgroundColor('#FFFFFF')
          .justifyContent(FlexAlign.Center)
          .alignItems(HorizontalAlign.Center)
        } else {
          List() {
            ForEach(this.sessions, (item: SessionDisplayItem) => {
              ListItem() {
                Row() {
                  Stack() {
                    Rect().fill('#07C160').width(48).height(48).borderRadius(8)
                    Text(item.name.charAt(0).toUpperCase())
                      .fontSize(20).fontColor('#FFFFFF').fontWeight(FontWeight.Bold)
                  }
                  .margin({ right: 12 })

                  Column() {
                    Text(item.name)
                      .fontSize(16)
                      .fontWeight(FontWeight.Medium)
                      .maxLines(1)
                    Row() {
                      Text('后端: ' + (this.backends.find(b => b.id === item.backendId)?.notes || item.backendUrl))
                        .fontSize(11)
                        .fontColor('#07C160')
                        .maxLines(1)
                        .textOverflow({ overflow: TextOverflow.Ellipsis })
                      Text(' | ')
                        .fontSize(11)
                        .fontColor('#CCCCCC')
                      Text(item.directory || '/')
                        .fontSize(11)
                        .fontColor('#888888')
                        .maxLines(1)
                        .textOverflow({ overflow: TextOverflow.Ellipsis })
                        .layoutWeight(1)
                    }
                    .margin({ top: 2 })
                    Text(item.updatedAt)
                      .fontSize(11)
                      .fontColor('#AAAAAA')
                      .margin({ top: 2 })
                  }
                  .layoutWeight(1)
                  .alignItems(HorizontalAlign.Start)

                  Text('配置')
                    .fontSize(13)
                    .fontColor('#1677FF')
                    .padding(8)
                    .onClick(() => {
                      const backend = this.backends.find(b => b.id === item.backendId);
                      router.pushUrl({
                        url: 'pages/SessionDetail',
                        params: {
                          backendId: item.backendId,
                          backendUrl: item.backendUrl,
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
                .backgroundColor('#FFFFFF')
              }
              .onClick(() => {
                this.core.setCurrentProject(item.backendId);
                this.core.setCurrentSession(item.id);
                router.pushUrl({
                  url: 'pages/ChatPage',
                  params: {
                    sessionId: item.id,
                    sessionName: item.name,
                    backendUrl: item.backendUrl,
                    authToken: this.backends.find(b => b.id === item.backendId)?.authToken || '',
                    directory: item.directory
                  }
                });
              })
            }, (item: SessionDisplayItem) => item.id)
          }
          .divider({ strokeWidth: 0.5, color: '#EEEEEE', startMargin: 76 })
          .backgroundColor('#FFFFFF')
          .flexGrow(1)
        }
      }
    }
    .width('100%')
    .backgroundColor('#EDEDED')
  }
}
