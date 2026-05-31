import { parseMarkdown, MarkdownSegment, MarkdownType } from '../utils/MessageParser';

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
  @StorageLink('themeAccent') themeAccent: string = '#07C160';
  @StorageLink('themeBgCard') themeBgCard: string = '#FFFFFF';
  @StorageLink('themeChatUserBg') themeChatUserBg: string = '#95EC69';
  @StorageLink('themeTextSecondary') themeTextSecondary: string = '#888888';
  @StorageLink('themeTextPrimary') themeTextPrimary: string = '#000000';
  @StorageLink('enableMarkdownRender') enableMarkdownRender: boolean = true;

  @Builder
  renderMarkdownContent(content: string) {
    Column() {
      ForEach(parseMarkdown(content), (segment: MarkdownSegment, index: number) => {
        if (segment.type === MarkdownType.CODE_BLOCK) {
          Column() {
            Text(segment.content)
              .fontSize(13)
              .fontColor(this.themeTextPrimary)
              .width('100%')
          }
          .width('100%')
          .padding(10)
          .backgroundColor('#F0F0F0')
          .borderRadius(8)
          .margin({ top: 4, bottom: 4 })
        } else {
          Text() {
            if (segment.type === MarkdownType.BOLD) {
              Span(segment.content).fontWeight(FontWeight.Bold).fontSize(15)
            } else if (segment.type === MarkdownType.CODE) {
              Span(segment.content)
                .fontSize(13)
                .fontColor('#D63384')
                .backgroundColor('#F5F5F5')
                .borderRadius(4)
            } else {
              Span(segment.content).fontSize(15)
            }
          }
          .width('100%')
        }
      }, (segment: MarkdownSegment, index: number) => `${index}_${segment.type}_${segment.content.length}`)
    }
    .width('100%')
  }

  build() {
    Row() {
      if (this.message.role === 'user') {
        Blank()
        Text(this.message.content)
          .padding(12)
          .backgroundColor(this.themeChatUserBg)
          .borderRadius({ topLeft: 8, topRight: 2, bottomLeft: 8, bottomRight: 8 })
          .constraintSize({ maxWidth: '75%' })
          .fontSize(15)
          .margin({ right: 12 })
      } else {
        Stack() {
          Rect().fill(this.themeAccent).width(36).height(36).borderRadius(4)
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
              .fontColor(this.themeTextSecondary)
              .margin({ left: 6 })
          }
          .padding(12)
          .backgroundColor(this.themeBgCard)
          .borderRadius({ topLeft: 2, topRight: 8, bottomLeft: 8, bottomRight: 8 })
        } else if (this.enableMarkdownRender) {
          Column() {
            this.renderMarkdownContent(this.message.content)
          }
          .padding(12)
          .backgroundColor(this.themeBgCard)
          .borderRadius({ topLeft: 2, topRight: 8, bottomLeft: 8, bottomRight: 8 })
          .constraintSize({ maxWidth: '75%' })
        } else {
          Text(this.message.content)
            .padding(12)
            .backgroundColor(this.themeBgCard)
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
