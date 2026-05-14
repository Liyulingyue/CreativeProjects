import { OpenCodeCore, OpenCodeProject } from '../core/OpenCodeCore';
import router from '@ohos.router';

interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
}

@Entry
@Component
struct ChatPage {
  @State project: OpenCodeProject | undefined = undefined;
  @State messages: ChatMessage[] = [];
  @State inputText: string = '';
  @State isLoading: boolean = false;
  private scroller: Scroller = new Scroller();
  private core: OpenCodeCore = OpenCodeCore.getInstance();

  aboutToAppear() {
    this.project = this.core.getCurrentProject();
    // 欢迎消息
    this.messages = [{
      id: '0',
      role: 'assistant',
      content: `已连接到 ${this.project?.name ?? '未知项目'}\n后端: ${this.project?.url ?? '-'}\n目录: ${this.project?.path ?? '-'}\n\n请输入指令开始工作。`,
      timestamp: Date.now()
    }];
  }

  build() {
    Column() {
      // 顶部导航栏
      Row() {
        Text('<')
          .fontSize(22)
          .fontColor('#07C160')
          .padding({ left: 16, right: 16, top: 12, bottom: 12 })
          .onClick(() => router.back())

        Column() {
          Text(this.project?.name ?? '项目')
            .fontSize(17)
            .fontWeight(FontWeight.Medium)
          Text(this.project?.url ?? '')
            .fontSize(11)
            .fontColor('#888888')
        }

        Blank()

        Text('···')
          .fontSize(22)
          .padding({ left: 16, right: 16, top: 12, bottom: 12 })
      }
      .width('100%')
      .backgroundColor('#EDEDED')

      // 消息列表
      Scroll(this.scroller) {
        Column({ space: 8 }) {
          ForEach(this.messages, (msg: ChatMessage) => {
            Row() {
              if (msg.role === 'user') {
                Blank()
                Text(msg.content)
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

                Text(msg.content)
                  .padding(12)
                  .backgroundColor('#FFFFFF')
                  .borderRadius({ topLeft: 2, topRight: 8, bottomLeft: 8, bottomRight: 8 })
                  .constraintSize({ maxWidth: '75%' })
                  .fontSize(15)
                Blank()
              }
            }
            .width('100%')
            .alignItems(VerticalAlign.Top)
          }, (msg: ChatMessage) => msg.id)

          if (this.isLoading) {
            Row() {
              Text('正在思考...')
                .padding(12)
                .backgroundColor('#FFFFFF')
                .borderRadius(8)
                .fontSize(14)
                .fontColor('#888888')
                .margin({ left: 56 })
            }.width('100%')
          }
        }
        .padding({ top: 12, bottom: 12 })
        .width('100%')
      }
      .flexGrow(1)
      .backgroundColor('#EDEDED')
      .scrollBar(BarState.Off)

      // 底部输入栏
      Row() {
        TextInput({ placeholder: '发送指令...', text: this.inputText })
          .flexGrow(1)
          .backgroundColor('#FFFFFF')
          .borderRadius(20)
          .padding({ left: 16, right: 16 })
          .onChange(v => this.inputText = v)

        Button('发送')
          .margin({ left: 8 })
          .backgroundColor('#07C160')
          .borderRadius(20)
          .enabled(!this.isLoading && this.inputText.length > 0)
          .onClick(() => this.sendMessage())
      }
      .padding({ left: 12, right: 12, top: 8, bottom: 20 })
      .backgroundColor('#F5F5F5')
    }
    .height('100%')
    .width('100%')
  }

  private async sendMessage() {
    const text = this.inputText.trim();
    if (!text) return;

    // 添加用户消息
    this.messages = [...this.messages, {
      id: Date.now().toString(),
      role: 'user',
      content: text,
      timestamp: Date.now()
    }];
    this.inputText = '';
    this.isLoading = true;

    // 滚动到底部
    this.scroller.scrollEdge(Edge.Bottom);

    try {
      const result = await this.core.sendCommand(text);
      this.messages = [...this.messages, {
        id: (Date.now() + 1).toString(),
        role: 'assistant',
        content: `执行成功: ${JSON.stringify(result)}`,
        timestamp: Date.now()
      }];
    } catch (e) {
      this.messages = [...this.messages, {
        id: (Date.now() + 1).toString(),
        role: 'assistant',
        content: `错误: ${e}`,
        timestamp: Date.now()
      }];
    } finally {
      this.isLoading = false;
      this.scroller.scrollEdge(Edge.Bottom);
    }
  }
}
