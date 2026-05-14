export interface DisplayMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
  isLoading?: boolean;
}

@Component
export struct MessageItem {
  @ObjectLink message: DisplayMessage;

  build() {
    Row() {
      if (this.message.role === 'user') {
        Blank()
        Text(this.message.content)
          .padding(12)
          .backgroundColor('#95EC69')
          .borderRadius({ topLeft: 8, topRight: 2, bottomLeft: 8, bottomRight: 8 })
          .constraintSize({ maxWidth: '75%' })
          .fontSize(15)
          .margin({ right: 12 })
      } else {
        Stack() {
          Rect().fill('#07C160').width(36).height(36).borderRadius(4)
          Text('AI')
            .fontSize(12).fontColor('#FFFFFF').fontWeight(FontWeight.Bold)
        }
        .margin({ left: 12, right: 8 })
        .alignContent(Alignment.Center)

        if (this.message.isLoading) {
          Row() {
            LoadingProgress().width(16).height(16)
            Text('思考中...')
              .fontSize(13)
              .fontColor('#888888')
              .margin({ left: 6 })
          }
          .padding(12)
          .backgroundColor('#FFFFFF')
          .borderRadius({ topLeft: 2, topRight: 8, bottomLeft: 8, bottomRight: 8 })
        } else {
          Text(this.message.content)
            .padding(12)
            .backgroundColor('#FFFFFF')
            .borderRadius({ topLeft: 2, topRight: 8, bottomLeft: 8, bottomRight: 8 })
            .constraintSize({ maxWidth: '75%' })
            .fontSize(15)
        }
        Blank()
      }
    }
    .width('100%')
    .alignItems(VerticalAlign.Top)
  }
}
