import { OpenCodeCore, OpenCodeMessage, OpenCodeMessagePart } from '../core/OpenCodeCore';
import router from '@ohos.router';
import http from '@ohos.net.http';

interface DisplayMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
  isLoading?: boolean;
}

interface ToolState {
  status?: string;
  title?: string;
  output?: string;
  error?: string;
}

interface FilePart {
  filename?: string;
}

interface TextPart {
  type: 'text';
  text: string;
}

interface MessageBody {
  parts: TextPart[];
}

@Entry
@Component
struct ChatPage {
  @State sessionId: string = '';
  @State sessionName: string = '会话';
  @State backendUrl: string = '';
  @State authToken: string = '';
  @State directory: string = '/';
  @State messages: DisplayMessage[] = [];
  @State inputText: string = '';
  @State isLoading: boolean = false;
  private scroller: Scroller = new Scroller();
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  private currentRequest: http.HttpRequest | null = null;

  aboutToAppear() {
    const params = router.getParams() as Record<string, Object>;
    if (params) {
      this.sessionId = (params['sessionId'] as string) || '';
      this.sessionName = (params['sessionName'] as string) || '会话';
      this.backendUrl = (params['backendUrl'] as string) || '';
      this.authToken = (params['authToken'] as string) || '';
      this.directory = (params['directory'] as string) || '/';
    }
    this.loadHistory();
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

  private cancelRequest() {
    if (this.currentRequest) {
      this.currentRequest.destroy();
      this.currentRequest = null;
    }
  }

  private getHeaders(): Record<string, string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'x-opencode-directory': encodeURIComponent(this.directory)
    };
    if (this.authToken) {
      headers['Authorization'] = 'Basic ' + this.base64Encode('opencode:' + this.authToken);
    }
    return headers;
  }

  private formatPartContent(part: OpenCodeMessagePart): string {
    switch (part.type) {
      case 'text':
        return part.text || '';
      case 'tool':
        const state = part.state as ToolState | undefined;
        if (state?.status === 'completed') {
          return `[Tool: ${state.title || part.tool}]\n${state.output || ''}`;
        } else if (state?.status === 'error') {
          return `[Tool Error: ${state.error || 'Unknown error'}]`;
        } else if (state?.status === 'running') {
          return `[Running: ${state.title || part.tool}...]`;
        } else {
          return `[Tool: ${part.tool || 'unknown'}]`;
        }
      case 'reasoning':
        return `<思考>\n${part.text || ''}\n</思考>`;
      case 'step-start':
        return `--- Step started ---`;
      case 'step-finish':
        return `--- Step completed ---`;
      case 'file':
        return `[File: ${(part as FilePart).filename || 'unknown'}]`;
      default:
        return '';
    }
  }

  private formatMessage(msg: OpenCodeMessage): string {
    const textParts = msg.parts
      .filter(p => p.type === 'text')
      .map(p => p.text || '')
      .join('\n');

    const otherParts = msg.parts
      .filter(p => p.type !== 'text')
      .map(p => this.formatPartContent(p))
      .filter(s => s.length > 0)
      .join('\n\n');

    const parts: string[] = [];
    if (textParts) parts.push(textParts);
    if (otherParts) parts.push(otherParts);

    if (msg.info.error) {
      parts.push(`[Error: ${msg.info.error.name}]`);
    }

    return parts.join('\n\n');
  }

  private async loadHistory() {
    if (!this.sessionId || !this.backendUrl) return;

    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = `${this.backendUrl.replace(/\/+$/, '')}/session/${encodeURIComponent(this.sessionId)}/message`;

    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.GET,
            header: this.getHeaders(),
            connectTimeout: 10000,
            readTimeout: 10000,
          },
          (err, data) => {
            if (err) reject(err);
            else resolve(data);
          }
        );
      });

      if (result.responseCode === 200) {
        const messages = JSON.parse(result.result as string) as OpenCodeMessage[];
        this.messages = messages.map((msg, index): DisplayMessage => ({
          id: msg.info.id || `msg-${index}`,
          role: msg.info.role as 'user' | 'assistant',
          content: this.formatMessage(msg),
          timestamp: msg.info.time.created
        }));
      }
    } catch (e) {
      console.error('[ChatPage] Load history error:', e);
    } finally {
      this.cancelRequest();
    }
  }

  private async sendMessage() {
    const text = this.inputText.trim();
    if (!text || !this.sessionId || !this.backendUrl || this.isLoading) return;

    this.messages = [...this.messages, {
      id: `user-${Date.now()}`,
      role: 'user',
      content: text,
      timestamp: Date.now()
    }];
    this.inputText = '';
    this.isLoading = true;
    this.scrollToBottom();

    const loadingId = `loading-${Date.now()}`;
    this.messages = [...this.messages, {
      id: loadingId,
      role: 'assistant',
      content: '思考中...',
      timestamp: Date.now(),
      isLoading: true
    }];

    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = `${this.backendUrl.replace(/\/+$/, '')}/session/${encodeURIComponent(this.sessionId)}/message`;

    const body: MessageBody = {
      parts: [{ type: 'text', text: text }]
    };

    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.POST,
            header: this.getHeaders(),
            extraData: JSON.stringify(body),
            connectTimeout: 120000,
            readTimeout: 120000,
          },
          (err, data) => {
            if (err) reject(err);
            else resolve(data);
          }
        );
      });

      this.messages = this.messages.filter(m => m.id !== loadingId);

      if (result.responseCode === 200) {
        const response = JSON.parse(result.result as string) as OpenCodeMessage;
        const content = this.formatMessage(response);
        this.messages = [...this.messages, {
          id: response.info.id || `resp-${Date.now()}`,
          role: 'assistant',
          content: content || '完成',
          timestamp: Date.now()
        }];
      } else {
        this.messages = [...this.messages, {
          id: `error-${Date.now()}`,
          role: 'assistant',
          content: `请求失败: HTTP ${result.responseCode}`,
          timestamp: Date.now()
        }];
      }
    } catch (e) {
      this.messages = this.messages.filter(m => m.id !== loadingId);
      this.messages = [...this.messages, {
        id: `error-${Date.now()}`,
        role: 'assistant',
        content: `错误: ${e}`,
        timestamp: Date.now()
      }];
    } finally {
      this.isLoading = false;
      this.cancelRequest();
      this.scrollToBottom();
    }
  }

  private scrollToBottom() {
    setTimeout(() => {
      this.scroller.scrollEdge(Edge.Bottom);
    }, 100);
  }

  build() {
    Column() {
      Row() {
        Text('<')
          .fontSize(22)
          .fontColor('#07C160')
          .padding({ left: 16, right: 16, top: 12, bottom: 12 })
          .onClick(() => router.back())

        Column() {
          Text(this.sessionName)
            .fontSize(17)
            .fontWeight(FontWeight.Medium)
            .maxLines(1)
          Text(this.directory || '/')
            .fontSize(11)
            .fontColor('#888888')
            .maxLines(1)
        }
        .layoutWeight(1)

        Blank()

        Text('···')
          .fontSize(22)
          .padding({ left: 16, right: 16, top: 12, bottom: 12 })
      }
      .width('100%')
      .height(56)
      .padding({ left: 8, right: 8 })
      .backgroundColor('#FFFFFF')

      Divider().height(0.5).color('#E0E0E0')

      Scroll(this.scroller) {
        Column({ space: 8 }) {
          ForEach(this.messages, (msg: DisplayMessage) => {
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

                if (msg.isLoading) {
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
                  Text(msg.content)
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
          }, (msg: DisplayMessage) => msg.id)
        }
        .padding({ top: 12, bottom: 12 })
        .width('100%')
      }
      .layoutWeight(1)
      .backgroundColor('#EDEDED')
      .scrollBar(BarState.Off)

      Row() {
        TextInput({ placeholder: '发送指令...', text: this.inputText })
          .layoutWeight(1)
          .backgroundColor('#FFFFFF')
          .borderRadius(20)
          .padding({ left: 16, right: 16 })
          .onChange(v => this.inputText = v)
          .onSubmit(() => this.sendMessage())

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
    .width('100%')
    .height('100%')
  }
}
