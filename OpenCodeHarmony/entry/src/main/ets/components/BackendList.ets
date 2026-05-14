import { OpenCodeCore, OpenCodeBackend } from '../core/OpenCodeCore';
import router from '@ohos.router';

@Component
export struct BackendList {
  @State backends: OpenCodeBackend[] = [];
  @StorageLink('backendRefreshTag') @Watch('onRefreshTagChange') private refreshTag: number = 0;
  private core: OpenCodeCore = OpenCodeCore.getInstance();

  aboutToAppear() {
    this.refreshBackends();
  }

  // 监听刷新标签变化
  onRefreshTagChange() {
    this.refreshBackends();
  }

  refreshBackends() {
    this.backends = [...this.core.getBackends()];
  }

  build() {
    Column() {
      Row() {
        Text('后端管理')
          .fontSize(20)
          .fontWeight(FontWeight.Bold)
        Blank()
        Text('+')
          .fontSize(28)
          .fontColor('#07C160')
          .padding({ left: 16, right: 16 })
          .onClick(() => {
            router.pushUrl({ url: 'pages/BackendDetail' });
          })
      }
      .width('100%')
      .padding({ left: 20, right: 4, top: 15, bottom: 15 })
      .backgroundColor('#EDEDED')

      if (this.backends.length === 0) {
        Column({ space: 16 }) {
          Stack() {
            Circle()
              .width(80)
              .height(80)
              .fill('#F0F5FF')
            Text('🖥')
              .fontSize(36)
          }
          .margin({ bottom: 8 })

          Text('暂无后端连接')
            .fontSize(18)
            .fontWeight(FontWeight.Medium)
            .fontColor('#333333')

          Text('添加后端地址以连接 OpenCode 服务')
            .fontSize(14)
            .fontColor('#999999')

          Button('添加后端', { type: ButtonType.Normal })
            .width(160)
            .height(44)
            .borderRadius(22)
            .backgroundColor('#07C160')
            .fontColor('#FFFFFF')
            .fontSize(15)
            .margin({ top: 8 })
            .onClick(() => {
              router.pushUrl({ url: 'pages/BackendDetail' });
            })
        }
        .width('100%')
        .height('100%')
        .backgroundColor('#FFFFFF')
        .justifyContent(FlexAlign.Center)
        .alignItems(HorizontalAlign.Center)
      } else {
        List() {
          ForEach(this.backends, (backend: OpenCodeBackend) => {
            ListItem() {
              Row() {
                Stack() {
                  Rect().fill('#07C160').width(48).height(48).borderRadius(8)
                  Text(backend.notes ? backend.notes.charAt(0).toUpperCase() : 'O')
                    .fontSize(20).fontColor('#FFFFFF').fontWeight(FontWeight.Bold)
                }
                .margin({ right: 12 })

                Column() {
                  Text(backend.notes ? backend.notes : backend.url)
                    .fontSize(15)
                    .fontWeight(FontWeight.Medium)
                    .maxLines(1)
                  if (backend.notes) {
                    Text(backend.url)
                      .fontSize(13)
                      .fontColor('#888888')
                      .margin({ top: 4 })
                      .maxLines(1)
                  }
                  if (backend.authToken) {
                    Text('已配置鉴权')
                      .fontSize(11)
                      .fontColor('#07C160')
                      .margin({ top: 4 })
                  }
                }
                .layoutWeight(1)
                .alignItems(HorizontalAlign.Start)

                Text('配置')
                  .fontSize(13)
                  .fontColor('#1677FF')
                  .padding(8)
                  .onClick(() => {
                    router.pushUrl({
                      url: 'pages/BackendDetail',
                      params: {
                        id: backend.id,
                        url: backend.url,
                        authToken: backend.authToken,
                        notes: backend.notes
                      }
                    });
                  })
              }
              .width('100%')
              .padding({ left: 16, right: 16, top: 12, bottom: 12 })
              .backgroundColor('#FFFFFF')
            }
            .onClick(() => {
              router.pushUrl({
                url: 'pages/BackendWebView',
                params: {
                  url: backend.url,
                  authToken: backend.authToken,
                  notes: backend.notes
                }
              });
            })
          }, (b: OpenCodeBackend) => b.id)
        }
        .divider({ strokeWidth: 0.5, color: '#EEEEEE', startMargin: 76 })
        .backgroundColor('#FFFFFF')
        .width('100%')
        .layoutWeight(1)
      }
    }
    .width('100%')
    .height('100%')
    .backgroundColor('#EDEDED')
  }
}
