import { OpenCodeCore, OpenCodeProject } from '../core/OpenCodeCore';
import router from '@ohos.router';

@Component
export struct SessionList {
  @State projects: OpenCodeProject[] = [];
  @StorageLink('sessionRefreshTag') private refreshTag: number = 0;
  private core: OpenCodeCore = OpenCodeCore.getInstance();

  aboutToAppear() {
    this.refreshProjects();
  }

  onPageShow() {
    this.refreshProjects();
  }

  refreshProjects() {
    this.projects = [...this.core.getProjects()];
  }

  build() {
    Column() {
      Row() {
        Text('OpenCode')
          .fontSize(20)
          .fontWeight(FontWeight.Bold)
        Blank()
        Image($r('app.media.startIcon'))
          .width(24).height(24).margin({ right: 20 })
        Text('+')
          .fontSize(28)
          .onClick(() => {
            router.pushUrl({ url: 'pages/SessionDetail' });
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

      if (this.projects.length === 0) {
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
              router.pushUrl({ url: 'pages/SessionDetail' });
            })
        }
        .width('100%')
        .height('100%')
        .backgroundColor('#FFFFFF')
        .justifyContent(FlexAlign.Center)
        .alignItems(HorizontalAlign.Center)
      } else {
        List() {
          ForEach(this.projects, (project: OpenCodeProject) => {
            ListItem() {
              Row() {
                Stack() {
                  Rect().fill('#07C160').width(48).height(48).borderRadius(8)
                  Text(project.name.charAt(0))
                    .fontSize(20).fontColor('#FFFFFF').fontWeight(FontWeight.Bold)
                }
                .margin({ right: 12 })

                Column() {
                  Text(project.notes || project.name)
                    .fontSize(16)
                    .fontWeight(FontWeight.Medium)
                    .maxLines(1)
                  if (project.notes) {
                    Text(project.name)
                      .fontSize(13)
                      .fontColor('#888888')
                      .margin({ top: 2 })
                      .maxLines(1)
                  }
                  Text(project.url)
                    .fontSize(12)
                    .fontColor('#AAAAAA')
                    .margin({ top: 2 })
                    .maxLines(1)
                }
                .flexGrow(1)
                .alignItems(HorizontalAlign.Start)
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
              this.core.setCurrentProject(project.id);
              router.pushUrl({ url: 'pages/ChatPage' });
            })
          }, (p: OpenCodeProject) => p.id)
        }
        .divider({ strokeWidth: 0.5, color: '#EEEEEE', startMargin: 76 })
        .backgroundColor('#FFFFFF')
        .flexGrow(1)
      }
    }
    .width('100%')
    .backgroundColor('#EDEDED')
  }
}
