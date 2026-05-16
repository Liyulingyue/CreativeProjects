import { OpenCodeCore, OpenCodeMessage, SseEvent } from '../core/OpenCodeCore';
import { ChatViewModel, DisplayMessage } from '../viewmodel/ChatViewModel';
import router from '@ohos.router';
import promptAction from '@ohos.promptAction';

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

interface PendingPermission {
  requestID: string;
  permission: string;
  patterns: string[];
  metadata?: Record<string, Object>;
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
  @State realSessionId: string = '';
  @State preferredModel: string = '';
  @State pendingPermission: PendingPermission | null = null;

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

    this.core.setSseEventCallback((event: SseEvent) => this.handleSseEvent(event));
    if (this.realSessionId) {
      this.core.startSse(this.sessionId);
    }

    this.loadHistory();
  }

  aboutToDisappear() {
    this.core.stopSse();
  }

  private handleSseEvent(event: SseEvent): void {
    if (event.type === 'permission.asked') {
      const props = event.properties;
      if (!props || !props.requestID) return;
      if (props.sessionID && props.sessionID !== this.realSessionId) return;
      const tools = props.metadata?.['tools'];
      const toolStr = Array.isArray(tools) ? (tools as string[]).join(', ') : '';
      console.info('[ChatPage] permission.asked:', props.permission, 'patterns:', props.patterns, 'tools:', toolStr);
      this.pendingPermission = {
        requestID: props.requestID || '',
        permission: props.permission || '',
        patterns: props.patterns || [],
        metadata: props.metadata,
      } as PendingPermission;
    }
  }

  private async replyPermission(response: 'once' | 'always' | 'reject'): Promise<void> {
    if (!this.pendingPermission || !this.realSessionId) return;
    const requestID = this.pendingPermission.requestID;
    this.pendingPermission = null;
    const success = await this.core.replyPermission(this.realSessionId, requestID, response);
    if (!success) {
      promptAction.showToast({ message: '审批回复失败' });
    }
  }

  private async loadHistory() {
    const pending = this.core.getPendingMessage();
    const hasPending = pending && pending.sessionId === this.realSessionId && pending.isLoading;
    if (hasPending) {
      console.info('[ChatPage] loadHistory: pending request detected, loadingId:', pending!.loadingId);
    }

    this.viewModel.loadHistory(
      this.backendUrl,
      this.authToken,
      this.directory,
      this.realSessionId,
      (msgs: DisplayMessage[]) => {
        this.messages = msgs;
        if (msgs.length > 0) {
          this.scrollToBottom();
        }
        if (hasPending) {
          this.isLoading = true;
          this.messages = [...this.messages, {
            id: pending!.loadingId,
            role: 'assistant',
            content: '思考中...',
            timestamp: Date.now(),
            isLoading: true
          }];
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
        .visibility(msg.isLoading ? Visibility.Visible : Visibility.None)

        Text(msg.content)
          .padding(12)
          .backgroundColor('#FFFFFF')
          .borderRadius({ topLeft: 2, topRight: 8, bottomLeft: 8, bottomRight: 8 })
          .constraintSize({ maxWidth: '75%' })
          .fontSize(15)
          .visibility(msg.isLoading ? Visibility.None : Visibility.Visible)
        Blank()
      }
    }
    .width('100%')
    .alignItems(VerticalAlign.Top)
    .padding({ left: 8, right: 8 })
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
                console.info('[ChatPage] Stop button clicked');
                this.core.cancelPendingRequest();
                this.isLoading = false;
                this.messages = this.messages.filter(m => !m.isLoading);
                return;
              }
              this.sendMessage();
            })
        }
      }
    }
    .width('100%')
    .height('100%')
    .bindSheet(!!this.pendingPermission, () => {
      this.buildPermissionSheet()
    }, {
      dragBar: true,
      showClose: false,
      preferType: SheetType.BOTTOM,
      maskColor: 'rgba(0,0,0,0.5)',
      onDisappear: () => {}
    })
  }

  @Builder
  buildPermissionSheet() {
    Column() {
      Row() {
        Text('权限请求')
          .fontSize(16)
          .fontWeight(FontWeight.Medium)
        Blank()
        Text('X')
          .fontSize(18)
          .fontColor('#888888')
          .onClick(() => { this.pendingPermission = null; })
      }
      .padding({ top: 16, bottom: 12, left: 20, right: 20 })

      Divider().color('#E0E0E0')

      Column() {
        Text(`权限类型: ${this.pendingPermission?.permission || ''}`)
          .fontSize(14)
          .fontColor('#333333')
          .width('100%')
        Text(`操作: ${this.pendingPermission?.patterns?.join(', ') || ''}`)
          .fontSize(13)
          .fontColor('#666666')
          .margin({ top: 6 })
          .width('100%')
      }
      .padding(20)
      .alignItems(HorizontalAlign.Start)
      .width('100%')

      Divider().color('#E0E0E0')

      Row() {
        Button('仅本次')
          .layoutWeight(1)
          .height(44)
          .backgroundColor('#F5F5F5')
          .fontColor('#333333')
          .onClick(() => this.replyPermission('once'))
        Divider().width(0.5).color('#E0E0E0').height(44)
        Button('始终允许')
          .layoutWeight(1)
          .height(44)
          .backgroundColor('#07C160')
          .fontColor('#FFFFFF')
          .onClick(() => this.replyPermission('always'))
        Divider().width(0.5).color('#E0E0E0').height(44)
        Button('拒绝')
          .layoutWeight(1)
          .height(44)
          .backgroundColor('#FF3B30')
          .fontColor('#FFFFFF')
          .onClick(() => this.replyPermission('reject'))
      }
    }
    .width('100%')
    .backgroundColor('#FFFFFF')
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
          this.core.startSse(this.sessionId);
        } else {
          throw new Error('Failed to create backend session');
        }
      } catch (e) {
        const err = e as Error;
        this.messages = [...this.messages, {
          id: `err-${Date.now()}`,
          role: 'assistant',
          content: `创建后端会话失败: ${err.message || String(e)}`,
          timestamp: Date.now()
        }];
        this.isLoading = false;
        return;
      }
    } else {
      this.core.startSse(this.sessionId);
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

    const currentPreferredModel = this.core.getProjectById(this.sessionId)?.preferredModel || '';

    this.core.sendMessagePersistent(
      this.sessionId,
      this.realSessionId,
      loadingId,
      text,
      currentPreferredModel,
      (result) => {
        const response = result.response;
        const error = result.error;
        const respLoadingId = result.loadingId;
        if (this.isLoading) {
          this.isLoading = false;
        }

        this.messages = this.messages.filter(m => m.id !== respLoadingId);

        if (error) {
          this.messages = [...this.messages, {
            id: `error-${Date.now()}`,
            role: 'assistant',
            content: error,
            timestamp: Date.now()
          }];
        } else if (response) {
          const content = this.viewModel.formatMessage(response);
          this.messages = [...this.messages, {
            id: response.info?.id || `resp-${Date.now()}`,
            role: 'assistant',
            content: content || '完成',
            timestamp: Date.now()
          }];
        }

        this.scrollToBottom();
      }
    );
  }
}
