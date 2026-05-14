import { OpenCodeSession } from '../core/OpenCodeCore';

@Component
export struct RemoteSessionPicker {
  @Prop sessions: OpenCodeSession[] = [];
  @Prop selectedId: string = '';
  @Prop isLoading: boolean = false;
  @Link showPicker: boolean;
  onSelect: (session: OpenCodeSession | null) => void = () => {};

  build() {
    Column() {
      List() {
        ListItem() {
          Text('不绑定 (开启全新对话)')
            .fontSize(14)
            .padding(12)
            .width('100%')
            .fontColor('#666666')
        }
        .onClick(() => {
          this.onSelect(null);
        })

        ForEach(this.sessions, (session: OpenCodeSession) => {
          ListItem() {
            Column() {
              Text(session.title || session.id)
                .fontSize(14)
                .fontColor('#333333')
                .maxLines(1)
              Text(`${session.directory} (${new Date(session.time.updated).toLocaleDateString()})`)
                .fontSize(11)
                .fontColor('#999999')
                .margin({ top: 2 })
            }
            .alignItems(HorizontalAlign.Start)
            .padding(12)
            .width('100%')
          }
          .onClick(() => {
            this.onSelect(session);
          })
        })
      }
      .divider({ strokeWidth: 0.5, color: '#EEEEEE' })
      .constraintSize({ maxHeight: 200 })
    }
    .width('100%')
    .backgroundColor('#FFFFFF')
    .borderRadius(8)
    .shadow({ radius: 10, color: '#10000000' })
    .margin({ top: 4 })
  }
}
