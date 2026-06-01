import { OpenCodeCore, OpenCodeMessage, SseEvent } from '../core/OpenCodeCore';
import { ChatViewModel, DisplayMessage } from '../viewmodel/ChatViewModel';
import { parseMessageSegments, MessageSegment, MessageSegmentType, parseMarkdown, MarkdownSegment, MarkdownType } from '../utils/MessageParser';
import router from '@ohos.router';

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
  @State collapsedIds: string[] = [];
  private scroller: Scroller = new Scroller();
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  private viewModel: ChatViewModel = new ChatViewModel();
  @State realSessionId: string = '';
  @State preferredModel: string = '';
  @State lastMsgCursor: string = '';
  @State lastCachedMsgTime: number = 0;
  @State pendingPermission: PendingPermission | null = null;
  @State isImmersiveMode: boolean = true;
  @State displayUsername: string = 'User';
  @StorageLink('themeAccent') themeAccent: string = '#07C160';
  @StorageLink('themeBgPrimary') themeBgPrimary: string = '#F5F5F5';
  @StorageLink('themeBgSecondary') themeBgSecondary: string = '#EDEDED';
  @StorageLink('themeBgCard') themeBgCard: string = '#FFFFFF';
  @StorageLink('themeTextPrimary') themeTextPrimary: string = '#333333';
  @StorageLink('themeTextSecondary') themeTextSecondary: string = '#888888';
  @StorageLink('themeTextTertiary') themeTextTertiary: string = '#999999';
  @StorageLink('themeTextMuted') themeTextMuted: string = '#BBBBBB';
  @StorageLink('themeBorderPrimary') themeBorderPrimary: string = '#E0E0E0';
  @StorageLink('themeLink') themeLink: string = '#007DFF';
  @StorageLink('themeStatusWorking') themeStatusWorking: string = '#2196F3';
  @StorageLink('themeStatusUnread') themeStatusUnread: string = '#FF3B30';
  @StorageLink('themeChatUserBg') themeChatUserBg: string = '#95EC69';
  @StorageLink('themeBorderSecondary') themeBorderSecondary: string = '#F0F0F0';
  @StorageLink('themeConfig') themeConfig: string = '#1677FF';
  @StorageLink('enableMarkdownRender') enableMarkdownRender: boolean = true;

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
    this.lastMsgCursor = '';
    this.lastCachedMsgTime = 0;
    this.core.setCurrentProject(this.sessionId);
    this.core.clearLeftFlag();
    if (this.realSessionId) {
      this.core.setCurrentSession(this.realSessionId);
    }

    // TODO: SSE 已禁用，避免干扰 HTTP 响应
    // this.core.setSseEventCallback((event: SseEvent) => this.handleSseEvent(event));
    // if (this.realSessionId) {
    //   this.core.startSse(this.sessionId);
    // }

    this.isImmersiveMode = this.core.getImmersiveMode();
    this.displayUsername = this.core.getDisplayUsername() || 'User';

    const draft = this.core.getDraft(this.sessionId);
    if (draft) {
      this.inputText = draft;
    }

    this.loadCachedMessages();
    this.loadHistory();
  }

  private async loadCachedMessages() {
    if (!this.realSessionId) return;
    try {
      const cached = await this.core.getCachedMessages(this.realSessionId);
      if (cached.length > 0) {
        this.messages = this.viewModel.toDisplayMessages(cached);
        const last = this.messages[this.messages.length - 1];
        if (last && last.msgId) {
          this.lastCachedMsgTime = last.timestamp;
          this.lastMsgCursor = this.viewModel.buildAfterCursor(last.msgId, last.timestamp);
        }
        console.info('[ChatPage] Loaded cached messages:', cached.length);
        this.scrollToBottom();
      }
    } catch (e) {
      console.error('[ChatPage] Failed to load cached messages:', e);
    }
  }

  aboutToDisappear() {
    this.core.saveDraft(this.sessionId, this.inputText);
    this.core.markCurrentProjectLeft();
    // TODO: SSE 已禁用
    // this.core.stopSse();
    AppStorage.setOrCreate<string>('refreshSessionsNow', Date.now().toString());
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
      AlertDialog.show({ message: '审批回复失败' });
    }
  }

  private async loadHistory() {
    const pending = this.core.getPendingMessage();
    const hasPending = pending && pending.sessionId === this.realSessionId && pending.isLoading;
    if (hasPending) {
      console.info('[ChatPage] loadHistory: pending request detected, loadingId:', pending!.loadingId);
    }

    const loadMode = this.core.getMessageLoadMode();

    if (loadMode === 'latest') {
      const page = await this.viewModel.loadHistoryPage(
        this.backendUrl,
        this.authToken,
        this.directory,
        this.realSessionId,
        undefined,
        20,
        'desc',
        this.username
      );
      console.info('[ChatPage] loadHistory got', page.items.length, 'items (latest mode)');
      if (page.items.length > 0) {
        this.messages = page.items;
        this.lastCachedMsgTime = page.items[page.items.length - 1].timestamp;
        if (page.cursor.next) {
          this.lastMsgCursor = this.viewModel.invertCursorForAfter(page.cursor.next);
        } else {
          const last = page.items[page.items.length - 1];
          if (last && last.msgId) {
            this.lastMsgCursor = this.viewModel.buildAfterCursor(last.msgId, last.timestamp);
          }
        }
        if (page.rawMessages && page.rawMessages.length > 0) {
          this.core.cacheMessages(this.realSessionId, page.rawMessages);
        }
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
    } else {
      this.viewModel.loadHistory(
        this.backendUrl,
        this.authToken,
        this.directory,
        this.realSessionId,
        (msgs: DisplayMessage[]) => {
          if (msgs.length > 0) {
            if (this.messages.length === 0) {
              this.messages = msgs;
            } else {
              this.messages = [...msgs, ...this.messages];
            }
            const last = this.messages[this.messages.length - 1];
            if (last && last.msgId) {
              this.lastMsgCursor = this.viewModel.buildAfterCursor(last.msgId, last.timestamp);
            }
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
        },
        this.username
      );
    }
  }

  private scrollToBottom() {
    setTimeout(() => {
      this.scroller.scrollEdge(Edge.Bottom);
    }, 100);
  }

  @Builder
  renderMarkdownText(content: string) {
    ForEach(parseMarkdown(content), (segment: MarkdownSegment, index: number) => {
      if (segment.type === MarkdownType.CODE_BLOCK) {
        Column() {
          Text(segment.content)
            .fontSize(12)
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

  @Builder
  buildMessageItem(msg: DisplayMessage) {
    Column() {
      if (!this.isImmersiveMode) {
        if (msg.role === 'user') {
          Row() {
            Blank()
            Text(this.formatTime(msg.timestamp))
              .fontSize(11)
              .fontColor(this.themeTextMuted)
            Text(this.displayUsername)
              .fontSize(11)
              .fontColor(this.themeTextSecondary)
              .margin({ left: 8 })
          }
          .width('100%')
          .padding({ left: 16, right: 16, top: 8, bottom: 2 })
        } else {
          Row() {
            if (msg.model) {
              Text(msg.model.split('/').pop() || msg.model)
                .fontSize(11)
                .fontColor(this.themeAccent)
              Text(this.formatTime(msg.timestamp))
                .fontSize(11)
                .fontColor(this.themeTextMuted)
                .margin({ left: 8 })
            } else {
              Text(this.formatTime(msg.timestamp))
                .fontSize(11)
                .fontColor(this.themeTextMuted)
            }
            Blank()
          }
          .width('100%')
          .padding({ left: 16, right: 16, top: 8, bottom: 2 })
        }
      }

      Row() {
        if (msg.role === 'user') {
          Blank()
          Text(msg.content)
            .copyOption(CopyOptions.LocalDevice)
            .padding(12)
            .backgroundColor(this.themeChatUserBg)
            .borderRadius({ topLeft: 8, topRight: 2, bottomLeft: 8, bottomRight: 8 })
            .constraintSize({ maxWidth: '75%' })
            .fontSize(15)
            .margin({ right: 12 })
        } else {
          Row() {
            Stack() {
              Rect().fill(this.themeAccent).width(36).height(36).borderRadius(4)
              Text('AI')
                .fontSize(12).fontColor('#FFFFFF').fontWeight(FontWeight.Bold)
            }
            .margin({ right: 8 })
            .alignContent(Alignment.Center)

            Column() {
              if (msg.isLoading) {
                Column() {
                  Row() {
                    LoadingProgress().width(16).height(16)
                    Text('思考中...')
                      .fontSize(13)
                      .fontColor(this.themeTextSecondary)
                      .margin({ left: 6 })
                  }
                  Text('如果长时间没有回复，请检查目标模型是否欠费或速率限制...')
                    .fontSize(10)
                    .fontColor(this.themeTextMuted)
                    .margin({ top: 6, left: 22 })
                }
                .padding(12)
                .backgroundColor(this.themeBgCard)
                .borderRadius({ topLeft: 2, topRight: 8, bottomLeft: 8, bottomRight: 8 })
                .alignItems(HorizontalAlign.Start)
              } else {
                Column() {
                  ForEach(parseMessageSegments(msg.content), (segment: MessageSegment, index: number) => {
                    if (segment.type === MessageSegmentType.TEXT) {
                      if (segment.content.trim().length > 0) {
                        if (this.enableMarkdownRender) {
                          Column() {
                            this.renderMarkdownText(segment.content)
                          }
                          .padding({ left: 12, right: 12, top: 10, bottom: 10 })
                        } else {
                          Text(segment.content)
                            .copyOption(CopyOptions.LocalDevice)
                            .fontSize(15)
                            .padding({ left: 12, right: 12, top: 10, bottom: 10 })
                        }
                      }
                    } else {
                      Column() {
                        Row() {
                          Text('执行过程' + (this.collapsedIds.indexOf(`${msg.id}_${index}`) === -1 ? ' (展开)' : ' (收起)'))
                            .fontSize(10)
                            .fontColor(this.themeTextTertiary)
                        }
                        .width('100%')
                        .justifyContent(FlexAlign.Center)
                        .padding({ top: 6, bottom: 6 })
                        .backgroundColor(this.themeBorderSecondary)
                        .borderRadius(4)
                        .onClick(() => {
                          const key = `${msg.id}_${index}`;
                          const idx = this.collapsedIds.indexOf(key);
                          if (idx === -1) {
                            this.collapsedIds.push(key);
                          } else {
                            this.collapsedIds.splice(idx, 1);
                          }
                        })

                        if (this.collapsedIds.indexOf(`${msg.id}_${index}`) !== -1) {
                          Text(segment.content)
                            .copyOption(CopyOptions.LocalDevice)
                            .fontSize(12)
                            .fontColor(this.themeTextSecondary)
                            .padding(10)
                            .width('100%')
                            .textAlign(TextAlign.Start)
                        }
                      }
                      .padding({ left: 10, right: 10, top: 4, bottom: 4 })
                    }
                  }, (segment: MessageSegment, index: number) => `${msg.id}_${index}`)
                }
                .backgroundColor(this.themeBgCard)
                .borderRadius({ topLeft: 2, topRight: 8, bottomLeft: 8, bottomRight: 8 })
                .alignItems(HorizontalAlign.Start)
              }
            }
            .layoutWeight(1)
            .constraintSize({ maxWidth: '80%' })
          }
          .width('100%')
          .padding({ left: 12, right: 12 })
          .alignItems(VerticalAlign.Top)

          Blank()
        }
      }
      .width('100%')
      .alignItems(VerticalAlign.Top)
    }
    .width('100%')
    .padding({ left: 8, right: 8 })
  }

  private formatTime(ts: number): string {
    const d = new Date(ts);
    const month = (d.getMonth() + 1).toString().padStart(2, '0');
    const day = d.getDate().toString().padStart(2, '0');
    const hour = d.getHours().toString().padStart(2, '0');
    const min = d.getMinutes().toString().padStart(2, '0');
    return `${month}-${day} ${hour}:${min}`;
  }

  build() {
    Column() {
      Row() {
        Text('<')
          .fontSize(22)
          .fontColor(this.themeAccent)
          .padding({ left: 16, right: 16, top: 12, bottom: 12 })
          .onClick(() => router.back())

        Column() {
          Text(this.sessionName)
            .fontSize(17)
            .fontWeight(FontWeight.Medium)
            .maxLines(1)
          Text(this.directory || '/')
            .fontSize(11)
            .fontColor(this.themeTextSecondary)
            .maxLines(1)
        }
        .alignItems(HorizontalAlign.Center)
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
      .backgroundColor(this.themeBgCard)
      .expandSafeArea([SafeAreaType.SYSTEM], [SafeAreaEdge.TOP])

      Divider().height(0.5).color(this.themeBorderPrimary)

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
      .backgroundColor(this.themeBgSecondary)
      .scrollBar(BarState.Off)
      .align(Alignment.Top)

      Column() {
        Row() {
          TextInput({ placeholder: '发送指令...', text: this.inputText })
            .layoutWeight(1)
            .backgroundColor(this.themeBgCard)
            .borderRadius(20)
            .padding({ left: 16, right: 16 })
            .onChange(v => this.inputText = v)
            .onSubmit(() => this.sendMessage())

          Button(this.isLoading ? '停止' : '发送')
            .margin({ left: 8 })
            .backgroundColor(this.themeAccent)
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
        .width('100%')
        .padding({ left: 8, right: 8, top: 8, bottom: 8 })
        .backgroundColor(this.themeBgCard)
      }
    }
    .width('100%')
    .height('100%')
    .bindSheet(!!this.pendingPermission, this.buildPermissionSheet, {
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
          .fontColor(this.themeTextSecondary)
          .onClick(() => { this.pendingPermission = null; })
      }
      .padding({ top: 16, bottom: 12, left: 20, right: 20 })

      Divider().color(this.themeBorderPrimary)

      Column() {
        Text(`权限类型: ${this.pendingPermission?.permission || ''}`)
          .fontSize(14)
          .fontColor(this.themeTextPrimary)
          .width('100%')
        Text(`操作: ${this.pendingPermission?.patterns?.join(', ') || ''}`)
          .fontSize(13)
          .fontColor(this.themeTextSecondary)
          .margin({ top: 6 })
          .width('100%')
      }
      .padding(20)
      .alignItems(HorizontalAlign.Start)
      .width('100%')

      Divider().color(this.themeBorderPrimary)

      Row() {
        Button('仅本次')
          .layoutWeight(1)
          .height(44)
          .backgroundColor(this.themeBgPrimary)
          .fontColor(this.themeTextPrimary)
          .onClick(() => this.replyPermission('once'))
        Divider().width(0.5).color(this.themeBorderPrimary).height(44)
        Button('始终允许')
          .layoutWeight(1)
          .height(44)
          .backgroundColor(this.themeAccent)
          .fontColor('#FFFFFF')
          .onClick(() => this.replyPermission('always'))
        Divider().width(0.5).color(this.themeBorderPrimary).height(44)
        Button('拒绝')
          .layoutWeight(1)
          .height(44)
          .backgroundColor(this.themeStatusUnread)
          .fontColor('#FFFFFF')
          .onClick(() => this.replyPermission('reject'))
      }
    }
    .width('100%')
    .backgroundColor(this.themeBgCard)
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
          // TODO: SSE 已禁用
          // this.core.startSse(this.sessionId);
          this.loadHistory();
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
      // TODO: SSE 已禁用
      // this.core.startSse(this.sessionId);
    }

    const userMsgId = `user-${Date.now()}`;
    this.messages = [...this.messages, {
      id: userMsgId,
      role: 'user',
      content: text,
      timestamp: Date.now()
    }];
    this.inputText = '';
    this.isLoading = true;
    this.scrollToBottom();

    const beforeCursor = this.lastMsgCursor;
    const loadingId = `loading-${Date.now()}`;
    this.messages = [...this.messages, {
      id: loadingId,
      role: 'assistant',
      content: '思考中...',
      timestamp: Date.now(),
      isLoading: true
    }];

    const currentPreferredModel = this.core.getProjectById(this.sessionId)?.preferredModel || '';
    const proj = this.core.getProjectById(this.sessionId);
    console.info('[ChatPage] calling sendMessagePersistent, sessionId:', this.sessionId, 'proj url:', proj?.url, 'username:', proj?.username);

    this.core.sendMessagePersistent(
      this.sessionId,
      this.realSessionId,
      loadingId,
      text,
      currentPreferredModel,
      async (result) => {
        const response = result.response;
        const error = result.error;
        const respLoadingId = result.loadingId;
        if (this.isLoading) {
          this.isLoading = false;
        }

        if (error) {
          this.messages = this.messages.filter(m => m.id !== respLoadingId);
          this.messages = [...this.messages, {
            id: `error-${Date.now()}`,
            role: 'assistant',
            content: error,
            timestamp: Date.now()
          }];
          this.scrollToBottom();
          return;
        }

        if (response) {
          let newMsgs: DisplayMessage[] = [];
          let newRawMsgs: OpenCodeMessage[] | undefined;
          if (beforeCursor) {
            const page = await this.viewModel.loadHistoryPage(
              this.backendUrl,
              this.authToken,
              this.directory,
              this.realSessionId,
              beforeCursor,
              50,
              'asc',
              this.username
            );
            newMsgs = page.items.filter(m => m.timestamp > this.lastCachedMsgTime);
            newRawMsgs = page.rawMessages?.filter(m => m.info?.time?.created > this.lastCachedMsgTime);
            if (page.cursor.next) {
              this.lastMsgCursor = this.viewModel.invertCursorForAfter(page.cursor.next);
            }
          }
          this.messages = this.messages.filter(m => m.id !== respLoadingId && m.id !== userMsgId);
          if (newMsgs.length > 0) {
            this.messages = [...this.messages, ...newMsgs];
          } else {
            this.messages = [...this.messages, {
              id: response.info?.id || `resp-${Date.now()}`,
              role: 'assistant',
              content: this.viewModel.formatMessage(response),
              timestamp: Date.now(),
              model: this.preferredModel
            }];
          }
          if (newRawMsgs && newRawMsgs.length > 0) {
            this.viewModel.appendCachedMessages(this.realSessionId, newRawMsgs);
          }
          this.scrollToBottom();
          this.core.clearDraft(this.sessionId);
        }
      }
    );
  }
}
