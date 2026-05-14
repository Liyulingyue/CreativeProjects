import { OpenCodeCore, OpenCodeMessage } from '../core/OpenCodeCore';
import { ChatViewModel, DisplayMessage } from '../viewmodel/ChatViewModel';
import router from '@ohos.router';
import http from '@ohos.net.http';
import util from '@ohos.util';

interface MessagePart {
  type: string;
  text: string;
}

interface ModelIdentifier {
  providerID: string;
  modelID: string;
}

interface MessageBody {
  parts: MessagePart[];
  model?: string | ModelIdentifier;
  messageID?: string;
  agent?: string;
}

interface SseEventStatus {
  type: string;
  attempt?: number;
  message?: string;
  next?: number;
}

interface SessionStatusEntry {
  type: string;
}

interface SseEventProperties {
  sessionID?: string;
  status?: SseEventStatus;
  error?: Record<string, Object>;
  part?: OpenCodeMessage; // reuse for part events
  delta?: string;
}

interface SseEvent {
  type: string;
  properties?: SseEventProperties;
  directory?: string;
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
  private viewModel: ChatViewModel = new ChatViewModel();
  private currentRequest: http.HttpRequest | null = null;
  @State realSessionId: string = '';
  private sseReq: http.HttpRequest | null = null;
  private sseBuffer: string = '';
  private readonly useSse: boolean = false;
  private readonly useEventDiagnostics: boolean = true;
  private loadingMsgId: string = '';
  private pollingTimer: number = -1;
  private diagnosticTimer: number = -1;
  private pollCount: number = 0;
  private assistantCountBeforePrompt: number = 0;
  private messageCountBeforePrompt: number = 0;
  private promptStartAt: number = 0;
  private currentPromptMessageId: string = '';
  private pendingPromptText: string = '';
  private lastRequestedModel: string = '(default)';
  private lastRequestedAgent: string = 'build';
  private diagnosticTriggered: boolean = false;
  private lastStatusType: string = 'unknown';
  private consecutiveIdleCount: number = 0;
  private hasSentSinceLoad: boolean = false;

  aboutToAppear() {
    this.hasSentSinceLoad = false;
    const params = router.getParams() as Record<string, Object>;
    if (params) {
      this.sessionId = (params['sessionId'] as string) || '';
      this.sessionName = (params['sessionName'] as string) || '会话';
      this.backendUrl = (params['backendUrl'] as string) || '';
      this.authToken = (params['authToken'] as string) || '';
      this.directory = (params['directory'] as string) || '/';
      const project = this.core.getProjectById(this.sessionId);
      if (project && project.remoteSessionId) {
        this.realSessionId = project.remoteSessionId;
        console.info('[ChatPage] Loaded existing remote session ID:', this.realSessionId);
      }
    }
    this.core.setCurrentProject(this.sessionId);
    if (this.realSessionId) {
      this.core.setCurrentSession(this.realSessionId);
    }
    this.loadHistory();
  }

  aboutToDisappear() {
    this.stopPolling();
    this.stopDiagnosticTimer();
    this.stopSse();
    this.cancelRequest();
  }

  private shouldUseEventStream(): boolean {
    return this.useSse || this.useEventDiagnostics;
  }

  private cancelRequest() {
    if (this.currentRequest) {
      this.currentRequest.destroy();
      this.currentRequest = null;
    }
  }

  private async loadHistory() {
    this.viewModel.loadHistory(
      this.backendUrl,
      this.authToken,
      this.directory,
      this.realSessionId,
      (msgs) => {
        // 页面进入后的首次历史回调若晚到，且用户已经发送过新消息，不要再覆盖当前列表
        if (this.hasSentSinceLoad) {
          if (this.shouldUseEventStream() && this.realSessionId) {
            this.startSse();
          }
          return;
        }

        // 若已在等待回复中，不覆盖当前消息列表（防止历史回调覆盖气泡）
        if (!this.isLoading) {
          this.messages = msgs;
          this.scrollToBottom();
        }
        if (this.shouldUseEventStream() && this.realSessionId) {
          this.startSse();
        }
      }
    );
  }

  private async sendMessage() {
    const text = this.inputText.trim();
    if (!text || !this.backendUrl || this.isLoading) return;
    this.hasSentSinceLoad = true;
    console.info('[ChatPage] sendMessage start, textLen=', text.length, 'sessionId=', this.realSessionId || '(new)');

    if (!this.realSessionId) {
      this.isLoading = true;
      try {
        const project = this.core.getProjectById(this.sessionId);
        const preferredModel = project?.preferredModel;
        const resp = await this.core.createBackendSession(this.backendUrl, this.authToken, this.directory, this.sessionName, preferredModel);
        if (resp && resp.id) {
          this.realSessionId = resp.id;
          this.core.setCurrentSession(resp.id);
          await this.core.updateRemoteSessionId(this.sessionId, resp.id);
          console.info('[ChatPage] Backend session created and linked:', this.realSessionId);
          // 新会话创建后立即建立 SSE 连接
          if (this.shouldUseEventStream()) {
            this.startSse();
          }
          console.info('[ChatPage] backend session ready:', this.realSessionId);
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
    } else if (this.shouldUseEventStream() && !this.sseReq) {
      // 已有 session 但 SSE 尚未建立（loadHistory 回调还没触发），立即启动
      this.startSse();
    }

    this.messages = [...this.messages, {
      id: `user-${Date.now()}`,
      role: 'user',
      content: text,
      timestamp: Date.now()
    }];
    this.inputText = '';
    this.isLoading = true;
    this.pendingPromptText = text;
    this.diagnosticTriggered = false;
    this.scrollToBottom();
    this.messageCountBeforePrompt = this.messages.filter(m => !m.isLoading).length;
    this.assistantCountBeforePrompt = this.messages
      .filter(m => m.role === 'assistant' && !m.isLoading && !!m.content && m.content.trim().length > 0)
      .length;

    const promptMessageId = `msg_${Date.now()}`;
    this.currentPromptMessageId = promptMessageId;
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
    const url = `${this.backendUrl.replace(/\/+$/, '')}/session/${encodeURIComponent(this.realSessionId)}/prompt_async`;

    const body = this.buildPromptBody(text, promptMessageId);
    console.info(
      '[ChatPage] prompt_async request prepared:',
      'messageID=', promptMessageId,
      'agent=', body.agent || '(none)',
      'model=', body.model ? JSON.stringify(body.model) : '(default)',
      'textPreview=', text.slice(0, 48)
    );

    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.POST,
            header: this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory),
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

      if (result.responseCode === 200 || result.responseCode === 204) {
        console.info('[ChatPage] prompt_async accepted, code=', result.responseCode);
        // 保留"思考中"气泡，轮询消息列表，检测到新助手回复后再替换
        this.loadingMsgId = loadingId;
        this.promptStartAt = Date.now();
        this.lastStatusType = 'unknown';
        this.consecutiveIdleCount = 0;
        this.startDiagnosticTimer();
        // 轮询作为主机制（当前默认关闭 SSE）
        this.startPolling();
      } else {
        console.warn('[ChatPage] prompt_async rejected, code=', result.responseCode, 'body=', result.result as string);
        this.messages = this.messages.filter(m => m.id !== loadingId);
        this.currentPromptMessageId = '';
        this.pendingPromptText = '';
        this.stopDiagnosticTimer();
        this.isLoading = false;
        this.messages = [...this.messages, {
          id: `error-${Date.now()}`,
          role: 'assistant',
          content: `请求失败: HTTP ${result.responseCode}`,
          timestamp: Date.now()
        }];
      }
    } catch (e) {
      console.error('[ChatPage] prompt_async exception:', e);
      this.isLoading = false;
      this.messages = this.messages.filter(m => m.id !== loadingId);
      this.currentPromptMessageId = '';
      this.pendingPromptText = '';
      this.stopDiagnosticTimer();
      this.messages = [...this.messages, {
        id: `error-${Date.now()}`,
        role: 'assistant',
        content: `错误: ${e}`,
        timestamp: Date.now()
      }];
    } finally {
      this.cancelRequest();
      this.scrollToBottom();
    }
  }

  private stopPolling() {
    if (this.pollingTimer !== -1) {
      clearTimeout(this.pollingTimer);
      this.pollingTimer = -1;
    }
  }

  private stopDiagnosticTimer() {
    if (this.diagnosticTimer !== -1) {
      clearTimeout(this.diagnosticTimer);
      this.diagnosticTimer = -1;
    }
  }

  private startDiagnosticTimer() {
    this.stopDiagnosticTimer();
    console.info('[ChatPage] diagnostic timer armed (15s) for messageID=', this.currentPromptMessageId);
    this.diagnosticTimer = setTimeout(() => {
      this.diagnosticTimer = -1;
      if (!this.isLoading || !this.loadingMsgId || this.diagnosticTriggered) return;
      this.diagnosticTriggered = true;
      console.warn('[ChatPage] diagnostic timer fired, running sync diagnostic for messageID=', this.currentPromptMessageId);
      this.runSyncDiagnostic();
    }, 15000);
  }

  private parsePreferredModel(model?: string): ModelIdentifier | undefined {
    if (!model) return undefined;
    const splitAt = model.indexOf('/');
    if (splitAt <= 0 || splitAt >= model.length - 1) {
      return undefined;
    }
    return {
      providerID: model.slice(0, splitAt),
      modelID: model.slice(splitAt + 1)
    };
  }

  private buildPromptBody(text: string, messageID: string): MessageBody {
    const project = this.core.getProjectById(this.sessionId);
    const preferredModel = project?.preferredModel;
    const parsedModel = this.parsePreferredModel(preferredModel);

    const body: MessageBody = {
      messageID,
      agent: 'build',
      parts: [{ type: 'text', text }]
    };

    if (parsedModel) {
      body.model = parsedModel;
    }

    this.lastRequestedAgent = body.agent || 'build';
    this.lastRequestedModel = body.model ? JSON.stringify(body.model) : '(default)';

    console.info(
      '[ChatPage] prompt body prepared:',
      'messageID=', messageID,
      'agent=', body.agent,
      'preferredModelRaw=', preferredModel || '(none)',
      'model=', body.model ? JSON.stringify(body.model) : '(default)'
    );

    return body;
  }

  private runSyncDiagnostic() {
    if (!this.realSessionId || !this.pendingPromptText) return;
    console.info('[ChatPage] runSyncDiagnostic start, sessionId=', this.realSessionId);

    const req = http.createHttp();
    const url = `${this.backendUrl.replace(/\/+$/, '')}/session/${encodeURIComponent(this.realSessionId)}/message`;
    const body = this.buildPromptBody(this.pendingPromptText, `msg_diag_${Date.now()}`);

    req.request(
      url,
      {
        method: http.RequestMethod.POST,
        header: this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory),
        extraData: JSON.stringify(body),
        connectTimeout: 15000,
        readTimeout: 120000,
      },
      (err, data) => {
        req.destroy();
        if (!this.loadingMsgId) return;

        if (err) {
          console.error('[ChatPage] runSyncDiagnostic transport error:', JSON.stringify(err));
          this.messages = [...this.messages, {
            id: `diag-error-${Date.now()}`,
            role: 'assistant',
            content: `诊断调用失败: ${JSON.stringify(err)}`,
            timestamp: Date.now()
          }];
          this.scrollToBottom();
          return;
        }

        if (data.responseCode !== 200) {
          console.warn('[ChatPage] runSyncDiagnostic non-200:', data.responseCode, 'body=', data.result as string);
          this.messages = [...this.messages, {
            id: `diag-http-${Date.now()}`,
            role: 'assistant',
            content: `诊断调用返回 HTTP ${data.responseCode}`,
            timestamp: Date.now()
          }];
          this.scrollToBottom();
          return;
        }

        try {
          const msg = JSON.parse(data.result as string) as OpenCodeMessage;
          const content = this.viewModel.formatMessage(msg);
          const assistantId = msg.info.id || `diag-assistant-${Date.now()}`;
          const assistantTs = msg.info.time.created || Date.now();
          const alreadyExists = this.messages.some(m => m.id === assistantId);
          const staleByTime = assistantTs < this.promptStartAt - 1000;
          const staleDiagnostic = alreadyExists || staleByTime;
          const responseModel = msg.info.model ? JSON.stringify(msg.info.model) : '(none)';
          console.info(
            '[ChatPage] runSyncDiagnostic success, assistantMsgId=', msg.info.id,
            'contentLen=', content.length,
            'requestedAgent=', this.lastRequestedAgent,
            'requestedModel=', this.lastRequestedModel,
            'responseModel=', responseModel,
            'staleDiagnostic=', staleDiagnostic,
            'alreadyExists=', alreadyExists,
            'staleByTime=', staleByTime
          );

          if (staleDiagnostic) {
            this.stopPolling();
            this.messages = this.messages.filter(m => m.id !== this.loadingMsgId);
            this.loadingMsgId = '';
            this.currentPromptMessageId = '';
            this.pendingPromptText = '';
            this.isLoading = false;
            this.messages = [...this.messages, {
              id: `diag-stale-${Date.now()}`,
              role: 'assistant',
              content: '模型请求已发送，但后端未返回新的助手消息（诊断返回了历史消息）。',
              timestamp: Date.now()
            }];
            this.scrollToBottom();
            return;
          }

          this.stopPolling();
          this.messages = this.messages.filter(m => m.id !== this.loadingMsgId);
          this.loadingMsgId = '';
          this.currentPromptMessageId = '';
          this.pendingPromptText = '';
          this.isLoading = false;
          if (content && content.trim().length > 0) {
            this.commitAssistantMessage(assistantId, content, assistantTs, 'sync-diagnostic');
          }
          this.scrollToBottom();
        } catch (e) {
          console.error('[ChatPage] runSyncDiagnostic parse error:', e);
          this.messages = [...this.messages, {
            id: `diag-parse-${Date.now()}`,
            role: 'assistant',
            content: `诊断响应解析失败: ${e}`,
            timestamp: Date.now()
          }];
          this.scrollToBottom();
        }
      }
    );
  }

  private commitAssistantMessage(id: string, content: string, timestamp: number, source: string) {
    const assistantMsg: DisplayMessage = {
      id,
      role: 'assistant',
      content,
      timestamp
    };

    // 去掉同 ID 的旧消息后再追加，确保 ForEach key 唯一且最后一条是最新助手回复
    const base = this.messages.filter(m => m.id !== id);
    this.messages = [...base, assistantMsg];

    const last = this.messages.length > 0 ? this.messages[this.messages.length - 1] : null;
    console.info(
      '[ChatPage] render-commit',
      'source=', source,
      'msgCount=', this.messages.length,
      'lastId=', last ? last.id : '(none)',
      'lastRole=', last ? last.role : '(none)',
      'lastLen=', last ? (last.content ? last.content.length : 0) : 0
    );
  }

  private mergeServerMessages(latestDisplayMsgs: DisplayMessage[]): DisplayMessage[] {
    if (latestDisplayMsgs.length === 0) {
      return this.messages.filter(m => !m.isLoading);
    }

    const latestIds: Record<string, boolean> = {};
    for (const msg of latestDisplayMsgs) {
      latestIds[msg.id] = true;
    }

    // 保留本地存在但服务端列表尚未出现的消息（常见于同步诊断返回后短暂未落库）。
    const localOnly = this.messages.filter(m => !m.isLoading && !latestIds[m.id]);
    if (localOnly.length === 0) {
      return latestDisplayMsgs;
    }

    const merged = [...latestDisplayMsgs, ...localOnly];
    merged.sort((a, b) => a.timestamp - b.timestamp);
    return merged;
  }

  // 轮询 /session/{id}/message，检测到新助手回复后完成
  private startPolling() {
    this.stopPolling();
    this.pollCount = 0;
    const INTERVAL_MS = 2000;
    const MAX_POLLS = 90; // 大约 3 分钟
    const poll = () => {
      const guardLoadingId = this.loadingMsgId;
      const guardPromptId = this.currentPromptMessageId;
      if (!guardLoadingId || !guardPromptId || !this.isLoading) {
        this.pollingTimer = -1;
        return;
      }

      const req = http.createHttp();
      const url = `${this.backendUrl.replace(/\/+$/, '')}/session/${encodeURIComponent(this.realSessionId)}/message`;
      req.request(
        url,
        {
          method: http.RequestMethod.GET,
          header: this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory),
          connectTimeout: 8000,
          readTimeout: 8000,
        },
        (err, data) => {
          req.destroy();
          if (!this.isLoading || this.loadingMsgId !== guardLoadingId || this.currentPromptMessageId !== guardPromptId) {
            return;
          }

          let completed = false;
          let latestDisplayMsgs: DisplayMessage[] = [];
          let hasAssistantAfterPrompt = false;
          if (!err && data.responseCode === 200) {
            try {
              const msgs = JSON.parse(data.result as string) as OpenCodeMessage[];
              latestDisplayMsgs = msgs.map((msg, i): DisplayMessage => ({
                id: msg.info.id || `msg-${i}`,
                role: msg.info.role as 'user' | 'assistant',
                content: this.viewModel.formatMessage(msg),
                timestamp: msg.info.time.created
              }));
              const assistantCountNow = latestDisplayMsgs
                .filter(m => m.role === 'assistant' && !!m.content && m.content.trim().length > 0)
                .length;
              const hasNewMessage = latestDisplayMsgs.length > this.messageCountBeforePrompt;
              const hasRecentAssistant = latestDisplayMsgs
                .some(m => m.role === 'assistant' && !!m.content && m.content.trim().length > 0 && m.timestamp >= this.promptStartAt - 1000);

              // 更稳的判定：找到本次 user message 后，是否出现 assistant 回复
              const promptIndex = msgs.findIndex(m => m.info.id === this.currentPromptMessageId);
              const assistantAfterPrompt = promptIndex >= 0
                ? msgs.slice(promptIndex + 1).find(m => {
                  if (m.info.role !== 'assistant') return false;
                  const content = this.viewModel.formatMessage(m);
                  return !!content && content.trim().length > 0;
                })
                : undefined;
              const hasAssistantAfterPromptByOrder = promptIndex >= 0
                && !!assistantAfterPrompt;

              hasAssistantAfterPrompt = hasAssistantAfterPromptByOrder
                || (assistantCountNow > this.assistantCountBeforePrompt)
                || (hasNewMessage && hasRecentAssistant);

              const promptIndexLog = msgs.findIndex(m => m.info.id === this.currentPromptMessageId);
              const latestRole = msgs.length > 0 ? msgs[msgs.length - 1].info.role : 'none';
              const responseModel = assistantAfterPrompt?.info?.model
                ? JSON.stringify(assistantAfterPrompt.info.model)
                : '(none)';
              console.info(
                '[ChatPage][Poll] message-scan',
                'promptIndex=', promptIndexLog,
                'msgTotal=', msgs.length,
                'assistantCountNow=', assistantCountNow,
                'assistantBefore=', this.assistantCountBeforePrompt,
                'hasRecentAssistant=', hasRecentAssistant,
                'hasAssistantAfterPrompt=', hasAssistantAfterPrompt,
                'latestRole=', latestRole,
                'requestedAgent=', this.lastRequestedAgent,
                'requestedModel=', this.lastRequestedModel,
                'responseModel=', responseModel
              );

              // 轮询期间持续同步服务端记录，同时保留本地 loading 气泡
              if (latestDisplayMsgs.length > 0 && this.loadingMsgId) {
                const mergedMsgs = this.mergeServerMessages(latestDisplayMsgs);
                const loadingMsg = this.messages.find(m => m.id === this.loadingMsgId);
                if (loadingMsg) {
                  this.messages = [...mergedMsgs, loadingMsg];
                } else {
                  this.messages = mergedMsgs;
                }
                this.scrollToBottom();
              }
            } catch (_) {
              // ignore parse error, continue polling
            }
          }

          // 补充查询 session/status，区分后端仍在计算还是已空闲
          const statusReq = http.createHttp();
          const statusUrl = `${this.backendUrl.replace(/\/+$/, '')}/session/status`;
          statusReq.request(
            statusUrl,
            {
              method: http.RequestMethod.GET,
              header: this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory),
              connectTimeout: 8000,
              readTimeout: 8000,
            },
            (statusErr, statusData) => {
              statusReq.destroy();
              if (!this.isLoading || this.loadingMsgId !== guardLoadingId || this.currentPromptMessageId !== guardPromptId) {
                return;
              }

              let statusType = this.lastStatusType;
              if (!statusErr && statusData.responseCode === 200) {
                try {
                  const map = JSON.parse(statusData.result as string) as Record<string, SessionStatusEntry>;
                  const entry = map[this.realSessionId];
                  statusType = entry ? entry.type : 'idle';
                } catch (_) {
                  // keep previous statusType
                }
              } else if (statusErr) {
                console.warn('[ChatPage][Poll] status request error:', JSON.stringify(statusErr));
              } else {
                console.warn('[ChatPage][Poll] status non-200:', statusData.responseCode);
              }
              this.lastStatusType = statusType;
              console.info(
                `[ChatPage][Poll] status=${statusType}, poll=${this.pollCount}, ` +
                `msgCount=${latestDisplayMsgs.length}, idleStreak=${this.consecutiveIdleCount}`
              );

              if (hasAssistantAfterPrompt) {
                this.messages = this.mergeServerMessages(latestDisplayMsgs);
                this.loadingMsgId = '';
                this.currentPromptMessageId = '';
                this.pendingPromptText = '';
                this.stopDiagnosticTimer();
                this.isLoading = false;
                this.pollingTimer = -1;
                this.scrollToBottom();
                console.info('[ChatPage][Poll] completed by new assistant message');
                completed = true;
              }

              if (completed) {
                return;
              }

              if (statusType === 'idle') {
                this.consecutiveIdleCount++;
              } else {
                this.consecutiveIdleCount = 0;
              }

              // 后端已连续空闲但没有产出助手消息，主动收尾并提示状态
              if (this.consecutiveIdleCount >= 2 && Date.now() - this.promptStartAt > 8000) {
                // 先尝试一次同步诊断调用，避免过早直接判定为“无回复”
                if (!this.diagnosticTriggered) {
                  this.diagnosticTriggered = true;
                  console.warn(
                    '[ChatPage][Poll] idle-without-assistant -> trigger sync diagnostic',
                    'idleStreak=', this.consecutiveIdleCount,
                    'elapsedMs=', Date.now() - this.promptStartAt,
                    'currentPromptMessageId=', this.currentPromptMessageId
                  );
                  this.runSyncDiagnostic();
                  this.pollingTimer = setTimeout(poll, INTERVAL_MS);
                  return;
                }

                console.warn(
                  '[ChatPage][Poll] closing by idle-without-assistant',
                  'idleStreak=', this.consecutiveIdleCount,
                  'elapsedMs=', Date.now() - this.promptStartAt,
                  'currentPromptMessageId=', this.currentPromptMessageId
                );
                this.pollingTimer = -1;
                this.messages = this.messages.filter(m => m.id !== this.loadingMsgId);
                this.loadingMsgId = '';
                this.currentPromptMessageId = '';
                this.pendingPromptText = '';
                this.stopDiagnosticTimer();
                this.isLoading = false;
                if (latestDisplayMsgs.length > 0) {
                  this.messages = latestDisplayMsgs;
                }
                this.messages = [...this.messages, {
                  id: `noresponse-${Date.now()}`,
                  role: 'assistant',
                  content: `后端状态已空闲（${statusType}），但未检测到新回复。`,
                  timestamp: Date.now()
                }];
                this.scrollToBottom();
                console.warn('[ChatPage][Poll] idle without new assistant message, closing loading state');
                return;
              }

              this.pollCount++;
              if (this.pollCount < MAX_POLLS) {
                this.pollingTimer = setTimeout(poll, INTERVAL_MS);
              } else {
                // 超时收尾
                this.pollingTimer = -1;
                this.messages = this.messages.filter(m => m.id !== this.loadingMsgId);
                this.loadingMsgId = '';
                this.currentPromptMessageId = '';
                this.pendingPromptText = '';
                this.stopDiagnosticTimer();
                this.isLoading = false;
                this.messages = [...this.messages, {
                  id: `timeout-${Date.now()}`,
                  role: 'assistant',
                  content: `请求超时，最后后端状态：${this.lastStatusType}`,
                  timestamp: Date.now()
                }];
                console.warn('[ChatPage][Poll] timeout, final status:', this.lastStatusType);
              }
            }
          );
        }
      );
    };
    this.pollingTimer = setTimeout(poll, INTERVAL_MS);
  }

  private stopSse() {
    if (this.sseReq) {
      this.sseReq.off('dataReceive');
      this.sseReq.off('dataEnd');
      this.sseReq.destroy();
      this.sseReq = null;
    }
    this.sseBuffer = '';
  }

  private startSse() {
    if (!this.shouldUseEventStream()) {
      return;
    }
    this.stopSse();
    const req = http.createHttp();
    const url = `${this.backendUrl.replace(/\/+$/, '')}/event`;
    const headers = this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory);
    headers['Accept'] = 'text/event-stream';

    req.on('dataReceive', (data: ArrayBuffer) => {
      try {
        const decoder = new util.TextDecoder('utf-8');
        const chunk = decoder.decode(new Uint8Array(data));
        this.handleSseChunk(chunk);
      } catch (e) {
        console.warn('[ChatPage][SSE] decode error:', e);
      }
    });

    req.on('dataEnd', () => {
      console.info('[ChatPage][SSE] stream ended, reconnecting in 3s...');
      this.sseReq = null;
      this.sseBuffer = '';
      // 自动重连
      setTimeout(() => {
        if (this.backendUrl && this.realSessionId) {
          this.startSse();
        }
      }, 3000);
    });

    req.requestInStream(
      url,
      {
        method: http.RequestMethod.GET,
        header: headers,
        connectTimeout: 10000,
        readTimeout: 0, // 0 = 永不超时，保持长连接
      },
      (err, responseCode) => {
        if (err) {
          console.warn('[ChatPage][SSE] requestInStream error:', JSON.stringify(err));
        } else {
          console.info('[ChatPage][SSE] connected, responseCode:', responseCode);
        }
      }
    );
    this.sseReq = req;
  }

  private handleSseChunk(chunk: string) {
    this.sseBuffer += chunk;
    // SSE 事件以 \n\n 分隔
    const events = this.sseBuffer.split('\n\n');
    // 最后一个可能不完整，保留
    this.sseBuffer = events.pop() ?? '';
    for (const raw of events) {
      const lines = raw.split('\n');
      for (const line of lines) {
        if (line.startsWith('data: ')) {
          const json = line.slice(6).trim();
          if (!json) continue;
          try {
            const event = JSON.parse(json) as SseEvent;
            this.onSseEvent(event);
          } catch (e) {
            console.warn('[ChatPage][SSE] JSON parse error:', e);
          }
        }
      }
    }
  }

  private onSseEvent(event: SseEvent) {
    if (!this.shouldUseEventStream()) {
      return;
    }
    const props = event.properties;

    if (event.type === 'session.error') {
      if (!props || !props.sessionID || props.sessionID !== this.realSessionId) return;
      const rawError = props.error ? JSON.stringify(props.error) : 'unknown error';
      console.error('[ChatPage][SSE] session.error:', rawError);
      this.stopPolling();
      this.stopDiagnosticTimer();
      if (this.loadingMsgId) {
        this.messages = this.messages.filter(m => m.id !== this.loadingMsgId);
      }
      this.loadingMsgId = '';
      this.currentPromptMessageId = '';
      this.pendingPromptText = '';
      this.isLoading = false;
      this.messages = [...this.messages, {
        id: `sse-error-${Date.now()}`,
        role: 'assistant',
        content: `后端执行失败: ${rawError}`,
        timestamp: Date.now()
      }];
      this.scrollToBottom();
      return;
    }

    if (event.type === 'session.status') {
      if (!props || !props.sessionID || props.sessionID !== this.realSessionId) return;
      const statusType = props.status?.type ?? 'idle';
      console.info('[ChatPage][SSE] session.status:', statusType, 'payload=', JSON.stringify(props.status));

      if (statusType === 'retry' && props.status?.message) {
        this.messages = [...this.messages, {
          id: `retry-${Date.now()}`,
          role: 'assistant',
          content: `后端重试中: ${props.status.message}`,
          timestamp: Date.now()
        }];
        this.scrollToBottom();
      }

      if (!this.useSse) {
        return;
      }

      if (statusType === 'idle') {
        // 只有正在等待回复时（loadingMsgId 非空）才处理 idle，避免初始状态的 idle 事件误操作
        if (!this.loadingMsgId) return;
        // SSE 优先到达：停止轮询，直接刷新
        // 不提前删气泡，由 fetchAndRefreshMessages 用服务端消息一次性替换
        this.stopPolling();
        this.stopDiagnosticTimer();
        this.loadingMsgId = '';
        this.currentPromptMessageId = '';
        this.pendingPromptText = '';
        this.isLoading = false;
        this.fetchAndRefreshMessages();
      } else if (statusType === 'busy') {
        this.isLoading = true;
      }
    }
  }

  // 拉取消息历史并刷新显示
  private fetchAndRefreshMessages() {
    const req = http.createHttp();
    const url = `${this.backendUrl.replace(/\/+$/, '')}/session/${encodeURIComponent(this.realSessionId)}/message`;
    req.request(
      url,
      {
        method: http.RequestMethod.GET,
        header: this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory),
        connectTimeout: 10000,
        readTimeout: 10000,
      },
      (err, data) => {
        req.destroy();
        if (err || data.responseCode !== 200) return;
        try {
          const msgs = JSON.parse(data.result as string) as OpenCodeMessage[];
          const displayMsgs: DisplayMessage[] = msgs.map((msg, i): DisplayMessage => ({
            id: msg.info.id || `msg-${i}`,
            role: msg.info.role as 'user' | 'assistant',
            content: this.viewModel.formatMessage(msg),
            timestamp: msg.info.time.created
          }));
          // 如果服务端没有返回任何助手消息内容，保留原消息列表并补充一条错误提示
          const hasContent = displayMsgs.some(m => m.role === 'assistant' && m.content && m.content.trim().length > 0);
          if (!hasContent && displayMsgs.length > 0) {
            // 只更新用户消息部分，追加一条错误提示
            const userMsgs = displayMsgs.filter(m => m.role === 'user');
            this.messages = [...userMsgs, {
              id: `noresponse-${Date.now()}`,
              role: 'assistant',
              content: '（未收到回复，可能触发了用量限制或网络超时）',
              timestamp: Date.now()
            }];
          } else {
            this.messages = displayMsgs;
          }
          this.scrollToBottom();
        } catch (e) {
          console.warn('[ChatPage] Failed to parse messages:', e);
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
