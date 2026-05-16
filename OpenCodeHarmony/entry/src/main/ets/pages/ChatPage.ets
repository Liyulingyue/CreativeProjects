import { OpenCodeCore, OpenCodeMessage } from '../core/OpenCodeCore';
import { ChatViewModel, DisplayMessage } from '../viewmodel/ChatViewModel';
import router from '@ohos.router';
import http from '@ohos.net.http';

interface MessagePart {
  type: string;
  text: string;
}

interface ModelRef {
  providerID: string;
  modelID: string;
}

interface MessageBody {
  parts: MessagePart[];
  model?: ModelRef;
  agent?: string;
}

interface ErrorDetail {
  message?: string;
}

interface ErrorResponse {
  error?: ErrorDetail;
  message?: string;
}

interface ErrorObject {
  code?: number;
  message?: string;
}

@Entry
@Component
struct ChatPage {
  @State sessionId: string = '';
  @State sessionName: string = '会话';
  @State backendUrl: string = '';
  @State username: string = '';
  @State authToken: string = '';
  @State directory: string = '/';
  @State messages: DisplayMessage[] = [];
  @State inputText: string = '';
  @State isLoading: boolean = false;
  private scroller: Scroller = new Scroller();
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  private viewModel: ChatViewModel = new ChatViewModel();
  private currentRequest: http.HttpRequest | null = null;
  @State realSessionId: string = '';
  @State preferredModel: string = '';

  aboutToAppear() {
    const params = router.getParams() as Record<string, Object>;
    if (params) {
      this.sessionId = (params['sessionId'] as string) || '';
      this.sessionName = (params['sessionName'] as string) || '会话';
      this.backendUrl = (params['backendUrl'] as string) || '';
      this.username = (params['username'] as string) || '';
      this.authToken = (params['authToken'] as string) || '';
      this.directory = (params['directory'] as string) || '/';
      const project = this.core.getProjectById(this.sessionId);
      console.info('[ChatPage] project from core:', JSON.stringify({
        id: project?.id,
        name: project?.name,
        preferredModel: project?.preferredModel,
        remoteSessionId: project?.remoteSessionId
      }));
      if (project && project.remoteSessionId) {
        this.realSessionId = project.remoteSessionId;
        console.info('[ChatPage] Loaded existing remote session ID:', this.realSessionId);
      }
      this.preferredModel = project?.preferredModel || '';
      console.info('[ChatPage] preferredModel set to:', this.preferredModel);
    }
    this.core.setCurrentProject(this.sessionId);
    if (this.realSessionId) {
      this.core.setCurrentSession(this.realSessionId);
    }
    this.loadHistory();
  }

  aboutToDisappear() {
    this.cancelRequest();
  }

  private cancelRequest() {
    if (this.currentRequest) {
      this.currentRequest.destroy();
      this.currentRequest = null;
    }
    if (this.realSessionId && this.backendUrl) {
      const abortUrl = `${this.backendUrl.replace(/\/+$/, '')}/session/${encodeURIComponent(this.realSessionId)}/abort`;
      http.createHttp().request(abortUrl, {
        method: http.RequestMethod.POST,
        header: this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory),
        connectTimeout: 5000,
        readTimeout: 5000,
      }).catch(() => {});
    }
  }

  private async loadHistory() {
    this.viewModel.loadHistory(
      this.backendUrl,
      this.authToken,
      this.directory,
      this.realSessionId,
      (msgs: DisplayMessage[]) => {
        if (!this.isLoading && msgs.length > 0) {
          this.messages = msgs;
          this.scrollToBottom();
        }
      }
    );
  }

  private scrollToBottom() {
    setTimeout(() => {
      this.scroller.scrollEdge(Edge.Bottom);
    }, 100);
  }

  @Builder
  buildMessageItem(msg: DisplayMessage) {
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
          Column() {
            Row() {
              LoadingProgress().width(16).height(16)
              Text('思考中...')
                .fontSize(13)
                .fontColor('#888888')
                .margin({ left: 6 })
            }
            Text('如果长时间没有回复，请检查目标模型是否欠费或速率限制...')
              .fontSize(10)
              .fontColor('#BBBBBB')
              .margin({ top: 6, left: 22 })
          }
          .padding(12)
          .backgroundColor('#FFFFFF')
          .borderRadius({ topLeft: 2, topRight: 8, bottomLeft: 8, bottomRight: 8 })
          .alignItems(HorizontalAlign.Start)
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
          .onClick(() => {
            const project = this.core.getProjectById(this.sessionId);
            router.pushUrl({
              url: 'pages/SessionDetail',
              params: {
                id: this.sessionId,
                title: this.sessionName,
                backendId: project?.backendId || '',
                backendUrl: this.backendUrl,
                username: this.username,
                authToken: this.authToken,
                directory: this.directory,
                isEditing: true
              }
            });
          })
      }
      .width('100%')
      .height(56)
      .padding({ left: 8, right: 8 })
      .backgroundColor('#FFFFFF')

      Divider().height(0.5).color('#E0E0E0')

      Scroll(this.scroller) {
        Column({ space: 8 }) {
          ForEach(this.messages, (msg: DisplayMessage) => {
            this.buildMessageItem(msg)
          }, (msg: DisplayMessage) => msg.id)
        }
        .padding({ top: 12, bottom: 12 })
        .width('100%')
      }
      .layoutWeight(1)
      .backgroundColor('#EDEDED')
      .scrollBar(BarState.Off)
      .align(Alignment.Top)

      Column() {
        Row() {
          TextInput({ placeholder: '发送指令...', text: this.inputText })
            .layoutWeight(1)
            .backgroundColor('#FFFFFF')
            .borderRadius(20)
            .padding({ left: 16, right: 16 })
            .onChange(v => this.inputText = v)
            .onSubmit(() => this.sendMessage())

          Button(this.isLoading ? '停止' : '发送')
            .margin({ left: 8 })
            .backgroundColor('#07C160')
            .borderRadius(20)
            .enabled(this.isLoading || this.inputText.length > 0)
            .onClick(() => {
              if (this.isLoading) {
                console.info('[ChatPage] Before cancelRequest, currentRequest:', this.currentRequest ? 'exists' : 'null');
                this.cancelRequest();
                console.info('[ChatPage] After cancelRequest');
                this.isLoading = false;
                this.messages = this.messages.filter(m => !m.isLoading);
                return;
              }
              this.sendMessage();
            })
        }
      }
      .padding({ left: 12, right: 12, top: 8, bottom: 12 })
      .backgroundColor('#F5F5F5')
    }
    .width('100%')
    .height('100%')
  }

  private async sendMessage() {
    console.info('[ChatPage] sendMessage called, isLoading:', this.isLoading, 'backendUrl:', !!this.backendUrl);
    const text = this.inputText.trim();
    if (!text || !this.backendUrl || this.isLoading) return;

    if (!this.realSessionId) {
      this.isLoading = true;
      try {
        const resp = await this.core.createBackendSession(this.backendUrl, this.username, this.authToken, this.directory, this.sessionName);
        if (resp && resp.id) {
          this.realSessionId = resp.id;
          this.core.setCurrentSession(resp.id);
          await this.core.updateRemoteSessionId(this.sessionId, resp.id);
          console.info('[ChatPage] Backend session created and linked:', this.realSessionId);
        } else {
          throw new Error('Failed to create backend session');
        }
      } catch (e) {
        this.messages = [...this.messages, {
          id: `err-${Date.now()}`,
          role: 'assistant',
          content: `创建后端会话失败: ${e.message || e}`,
          timestamp: Date.now()
        }];
        this.isLoading = false;
        return;
      }
    }

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
    const url = `${this.backendUrl.replace(/\/+$/, '')}/session/${encodeURIComponent(this.realSessionId)}/message`;

    const currentPreferredModel = this.core.getProjectById(this.sessionId)?.preferredModel || '';
    let model: ModelRef | undefined;
    if (currentPreferredModel && currentPreferredModel.includes('/')) {
      const parts = currentPreferredModel.split('/');
      model = { providerID: parts[0], modelID: parts[1] };
    }

    const body: MessageBody = { parts: [{ type: 'text', text: text }], model: model };

    console.info('[ChatPage] >>> sendMessage params:');
    console.info('[ChatPage]   preferredModel (fresh):', JSON.stringify(currentPreferredModel));
    console.info('[ChatPage]   realSessionId:', this.realSessionId);
    console.info('[ChatPage]   model:', JSON.stringify(model));
    console.info('[ChatPage]   agent:', 'build');
    console.info('[ChatPage] body:', JSON.stringify(body));

    console.info('[ChatPage] About to enter try block');
    let result: http.HttpResponse | null = null;
    try {
      console.info('[ChatPage] Inside try block, creating request');
      console.info('[ChatPage] Creating HTTP request, URL:', url);
      console.info('[ChatPage] Headers:', JSON.stringify(this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory)));

      result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.POST,
            header: this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory),
            extraData: JSON.stringify(body),
            connectTimeout: 60000,
            readTimeout: 0,
          },
          (err, data) => {
            console.info('[ChatPage] HTTP callback, err:', JSON.stringify(err), 'responseCode:', data?.responseCode, 'result len:', (data?.result as string)?.length);
            if (err) {
              console.error('[ChatPage] HTTP error:', err);
              reject(err);
            } else {
              resolve(data);
            }
          }
        );
        console.info('[ChatPage] HTTP request issued');
      });

      console.info('[ChatPage] HTTP response, code:', result.responseCode, 'result:', (result.result as string)?.slice(0, 300));

      this.messages = this.messages.filter(m => m.id !== loadingId);

      if (result.responseCode === 200) {
        const response: OpenCodeMessage = JSON.parse(result.result as string) as OpenCodeMessage;
        const content = this.viewModel.formatMessage(response);
        this.messages = [...this.messages, {
          id: response.info?.id || `resp-${Date.now()}`,
          role: 'assistant',
          content: content || '完成',
          timestamp: Date.now()
        }];
      } else {
        let errMsg = '';
        if (result.responseCode === 429) {
          try {
            const body = JSON.parse(result.result as string) as ErrorResponse;
            errMsg = body?.error?.message || body?.message || '请求频率超限';
          } catch {
            errMsg = '请求频率超限';
          }
        } else {
          errMsg = `请求失败: HTTP ${result.responseCode}`;
        }
        this.messages = [...this.messages, {
          id: `error-${Date.now()}`,
          role: 'assistant',
          content: errMsg,
          timestamp: Date.now()
        }];
      }
    } catch (e) {
      this.messages = this.messages.filter(m => m.id !== loadingId);
      let errMsg = '';
      if (result && result.responseCode === 429) {
        try {
          const body = JSON.parse(result.result as string) as ErrorResponse;
          errMsg = body?.error?.message || body?.message || '请求频率超限';
        } catch {
          errMsg = '请求频率超限';
        }
      } else {
        const errObj = e as ErrorObject;
        errMsg = errObj?.message || (typeof e === 'object' ? JSON.stringify(e) : String(e));
      }
      this.messages = [...this.messages, {
        id: `error-${Date.now()}`,
        role: 'assistant',
        content: errMsg || `请求失败`,
        timestamp: Date.now()
      }];
    } finally {
      this.isLoading = false;
      this.cancelRequest();
      this.scrollToBottom();
    }
  }
}
