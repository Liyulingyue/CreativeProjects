import { OpenCodeCore, OpenCodeMessage, OpenCodeMessagePart } from '../core/OpenCodeCore';
import { ChatViewModel, DisplayMessage } from '../viewmodel/ChatViewModel';
import router from '@ohos.router';
import http from '@ohos.net.http';
import util from '@ohos.util';

interface OpenCodeMessageModel {
  providerID: string;
  modelID: string;
}

interface OpenCodeMessageTime {
  created: number;
  completed?: number;
}

interface OpenCodeMessageError {
  name: string;
  data: Record<string, Object>;
}

interface OpenCodeMessageInfo {
  id: string;
  sessionID: string;
  role: 'user' | 'assistant';
  agent?: string;
  model?: OpenCodeMessageModel;
  time: OpenCodeMessageTime;
  error?: OpenCodeMessageError;
}

interface MessagePart {
  id: string;
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

const DEFAULT_PROMPT_AGENT = 'build';

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
  info?: OpenCodeMessageInfo;
  part?: OpenCodeMessagePart;
  messageID?: string;
  partID?: string;
  field?: string;
  delta?: string;
}

interface SseEvent {
  type: string;
  properties?: SseEventProperties;
  directory?: string;
}

interface LogExtrasPrepare {
  textLen: number;
  requestedModel: string;
  requestedAgent: string;
}

interface LogExtrasAccepted {
  httpCode: number;
  transport: string;
}

interface LogExtrasConnect {
  responseCode: number;
}

interface LogExtrasStatus {
  statusType: string;
}

interface LogExtrasIgnored {
  eventType: string;
  hasProps: boolean;
}

interface LogExtrasFetch {
  total: number;
  promptIndex: number;
  hasAssistantAfterPrompt: boolean;
  responseModel: string;
}

type LogExtras = LogExtrasPrepare | LogExtrasAccepted | LogExtrasConnect | LogExtrasStatus | LogExtrasIgnored | LogExtrasFetch;

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
  private readonly useSse: boolean = true;
  private loadingMsgId: string = '';
  private promptStartAt: number = 0;
  private idCounter: number = 0;
  private currentPromptMessageId: string = '';
  private promptAccepted: boolean = false;
  private eventFetchTimer: number = -1;
  private sseReconnectTimer: number = -1;
  private lastRequestedModel: string = '(default)';
  private lastRequestedAgent: string = '(default)';
  private lastStatusType: string = 'unknown';
  private hasSentSinceLoad: boolean = false;
  private messageInfos: Map<string, OpenCodeMessageInfo> = new Map();
  private messageParts: Map<string, OpenCodeMessagePart[]> = new Map();

  private logAlign(stage: string, extras?: LogExtras) {
    const base = [
      '[ChatPage][Align]',
      `stage=${stage}`,
      `session=${this.realSessionId || '(none)'}`,
      `useSse=${this.useSse}`,
      `loading=${this.isLoading}`,
      `loadingMsgId=${this.loadingMsgId || '(none)'}`,
      `promptMsgId=${this.currentPromptMessageId || '(none)'}`,
      `lastStatus=${this.lastStatusType}`
    ];
    if (extras) {
      // 使用类型判断处理不同类型的 extras
      const extrasStr = JSON.stringify(extras);
      base.push(`extras=${extrasStr}`);
    }
    console.info(base.join(' '));
  }

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
    if (this.eventFetchTimer !== -1) {
      clearTimeout(this.eventFetchTimer);
      this.eventFetchTimer = -1;
    }
    if (this.sseReconnectTimer !== -1) {
      clearTimeout(this.sseReconnectTimer);
      this.sseReconnectTimer = -1;
    }
    this.stopSse();
    this.cancelRequest();
  }

  private shouldUseEventStream(): boolean {
    return this.useSse;
  }

  private cancelRequest() {
    if (this.currentRequest) {
      this.currentRequest.destroy();
      this.currentRequest = null;
    }
  }

  private clearPendingState(removeLoadingBubble: boolean = true) {
    if (this.eventFetchTimer !== -1) {
      clearTimeout(this.eventFetchTimer);
      this.eventFetchTimer = -1;
    }
    if (removeLoadingBubble && this.loadingMsgId) {
      this.messages = this.messages.filter(m => m.id !== this.loadingMsgId);
    }
    this.loadingMsgId = '';
    this.currentPromptMessageId = '';
    this.promptAccepted = false;
    this.isLoading = false;
  }

  private scheduleSseReconnect(delayMs: number = 3000) {
    if (!this.backendUrl || !this.realSessionId) {
      return;
    }
    if (this.sseReconnectTimer !== -1) {
      clearTimeout(this.sseReconnectTimer);
    }
    this.sseReconnectTimer = setTimeout(() => {
      this.sseReconnectTimer = -1;
      if (this.backendUrl && this.realSessionId) {
        this.startSse();
      }
    }, delayMs) as number;
  }

  private scheduleEventFetchFallback(delayMs: number = 10000) {
    if (this.eventFetchTimer !== -1) {
      clearTimeout(this.eventFetchTimer);
      this.eventFetchTimer = -1;
    }

    const promptMessageId = this.currentPromptMessageId;
    const loadingMessageId = this.loadingMsgId;
    if (!promptMessageId || !loadingMessageId) {
      return;
    }

    this.eventFetchTimer = setTimeout(() => {
      this.eventFetchTimer = -1;
      if (!this.promptAccepted || !this.loadingMsgId) {
        return;
      }
      // 只在完全没有收到状态事件时兜底，避免干扰正常 busy -> idle 流程
      if (this.lastStatusType !== 'unknown') {
        return;
      }
      this.logAlign('prompt_async.fallback.fetch');
      this.fetchAndRefreshMessages(promptMessageId, loadingMessageId, 0);
    }, delayMs) as number;
  }

  private abortSessionBestEffort(stage: string) {
    if (!this.realSessionId || !this.backendUrl) {
      return;
    }

    const req = http.createHttp();
    const url = `${this.backendUrl.replace(/\/+$/, '')}/session/${encodeURIComponent(this.realSessionId)}/abort`;
    this.logAlign(`abort.request.${stage}`);
    req.request(
      url,
      {
        method: http.RequestMethod.POST,
        header: this.viewModel.getHeaders(this.backendUrl, this.authToken, this.directory),
        connectTimeout: 10000,
        readTimeout: 10000,
      },
      (err, data) => {
        if (err) {
          console.warn('[ChatPage] abort request failed:', JSON.stringify(err));
          this.logAlign(`abort.error.${stage}`);
        } else {
          console.info('[ChatPage] abort request sent, code=', data.responseCode);
          this.logAlign(`abort.ok.${stage}`);
        }
        req.destroy();
      }
    );
  }

  private stopCurrentRunByUser() {
    this.clearPendingState(true);
    this.abortSessionBestEffort('user');
    this.scrollToBottom();
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
        // 按时间倒序排列（最新的在前面，这样 ForEach 渲染后最新的在底部）
        msgs.sort((a, b) => b.timestamp - a.timestamp);
        this.messages = msgs;
        this.scrollToBottom();
      }
        if (this.shouldUseEventStream() && this.realSessionId) {
          this.startSse();
        }
      }
    );
  }

  private resetMessageCacheFromServer(msgs: OpenCodeMessage[]) {
    this.messageInfos = new Map();
    this.messageParts = new Map();
    for (const msg of msgs) {
      if (!msg.info?.id) continue;
      this.messageInfos.set(msg.info.id, msg.info);
      this.messageParts.set(msg.info.id, [...(msg.parts || [])]);
    }
  }

  private upsertMessageInfo(info?: OpenCodeMessageInfo) {
    if (!info?.id) return;
    this.messageInfos.set(info.id, info);
    if (!this.messageParts.has(info.id)) {
      this.messageParts.set(info.id, []);
    }
  }

  private upsertMessagePart(part?: OpenCodeMessagePart) {
    if (!part?.messageID || !part?.id) return;
    const list = this.messageParts.get(part.messageID) || [];
    const idx = list.findIndex(p => p.id === part.id);
    if (idx >= 0) {
      list[idx] = part;
    } else {
      list.push(part);
    }
    this.messageParts.set(part.messageID, list);
  }

  private applyMessagePartDelta(messageID?: string, partID?: string, field?: string, delta?: string) {
    if (!messageID || !partID || !field || !delta) return;
    const list = this.messageParts.get(messageID) || [];
    const idx = list.findIndex(p => p.id === partID);
    if (idx < 0) return;
    const part = list[idx];
    // 使用条件判断处理不同字段，避免索引访问
    if (field === 'text' && part.text !== undefined) {
      part.text = `${part.text}${delta}`;
    }
    this.messageParts.set(messageID, list);
  }

  private removeMessagePart(messageID?: string, partID?: string) {
    if (!messageID || !partID) return;
    const list = this.messageParts.get(messageID) || [];
    this.messageParts.set(messageID, list.filter(p => p.id !== partID));
  }

  private removeMessageInfo(messageID?: string) {
    if (!messageID) return;
    this.messageInfos.delete(messageID);
    this.messageParts.delete(messageID);
  }

  private rebuildDisplayFromCache() {
    const ids = Array.from(this.messageInfos.keys());
    const cacheMsgs: OpenCodeMessage[] = ids
      .map((id): OpenCodeMessage => ({
        info: this.messageInfos.get(id)!,
        parts: this.messageParts.get(id) || []
      }))
      .sort((a, b) => b.info.time.created - a.info.time.created);

    // 只有当缓存有消息时才替换，否则保留当前列表（防止思考中被顶到顶部）
    if (cacheMsgs.length > 0) {
      this.messages = cacheMsgs.map((msg, i): DisplayMessage => ({
        id: msg.info.id || `msg-${i}`,
        role: msg.info.role as 'user' | 'assistant',
        content: this.viewModel.formatMessage(msg),
        timestamp: msg.info.time.created
      }));
    }

    if (this.loadingMsgId && !this.messages.find(m => m.id === this.loadingMsgId)) {
      this.messages = [...this.messages, {
        id: this.loadingMsgId,
        role: 'assistant',
        content: '思考中...',
        timestamp: Date.now(),
        isLoading: true
      }];
    }
  }

  private async sendMessage() {
    const text = this.inputText.trim();
    if (!text) {
      if (this.isLoading) {
        this.stopCurrentRunByUser();
      }
      return;
    }
    if (!this.backendUrl || this.isLoading) return;
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

    const promptMessageId = this.createMessageId();

    this.messages = [...this.messages, {
      id: promptMessageId,
      role: 'user',
      content: text,
      timestamp: Date.now()
    }];
    this.inputText = '';
    this.isLoading = true;
    this.scrollToBottom();

    this.currentPromptMessageId = promptMessageId;
    this.promptAccepted = false;
    const loadingId = `loading-${Date.now()}`;
    this.loadingMsgId = loadingId;
    this.promptStartAt = Date.now();
    console.info('[ChatPage] sendMessage set loadingMsgId=', loadingId, 'messages count before=', this.messages.length);
    this.messages = [...this.messages, {
      id: loadingId,
      role: 'assistant',
      content: '思考中...',
      timestamp: Date.now(),
      isLoading: true
    }];
    console.info('[ChatPage] sendMessage messages count after=', this.messages.length);

    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = `${this.backendUrl.replace(/\/+$/, '')}/session/${encodeURIComponent(this.realSessionId)}/prompt_async`;

    const body = this.buildPromptBody(text, promptMessageId);
    this.logAlign('prompt_async.prepare', {
      textLen: text.length,
      requestedModel: this.lastRequestedModel,
      requestedAgent: this.lastRequestedAgent
    });
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
        // 保留"思考中"气泡，主流程仅走 SSE
        this.lastStatusType = 'unknown';
        this.promptAccepted = true;
        this.scheduleEventFetchFallback(10000);
        this.logAlign('prompt_async.accepted', {
          httpCode: result.responseCode,
          transport: 'sse'
        });
        if (!this.sseReq) {
          this.startSse();
        }
      } else {
        console.warn('[ChatPage] prompt_async rejected, code=', result.responseCode, 'body=', result.result as string);
        this.clearPendingState(true);
        this.abortSessionBestEffort('prompt_async.rejected');
        this.messages = [...this.messages, {
          id: `error-${Date.now()}`,
          role: 'assistant',
          content: `请求失败: HTTP ${result.responseCode}`,
          timestamp: Date.now()
        }];
      }
    } catch (e) {
      console.error('[ChatPage] prompt_async exception:', e);
      this.clearPendingState(true);
      this.abortSessionBestEffort('prompt_async.exception');
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

  private randomBase62(length: number): string {
    const chars = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
    let result = '';
    for (let i = 0; i < length; i += 1) {
      result += chars.charAt(Math.floor(Math.random() * chars.length));
    }
    return result;
  }

  private createAscendingId(prefix: 'msg' | 'prt'): string {
    this.idCounter += 1;
    const timeHex = Date.now().toString(16).padStart(12, '0');
    const countHex = (this.idCounter % 0x1000).toString(16).padStart(3, '0');
    // 26 位尾部与 web 的 ID 形态一致，降低后端兼容风险。
    return `${prefix}_${timeHex}${countHex}${this.randomBase62(11)}`;
  }

  private createMessageId(): string {
    return this.createAscendingId('msg');
  }

  private createPartId(): string {
    return this.createAscendingId('prt');
  }

  private buildPromptBody(text: string, messageID: string): MessageBody {
    const project = this.core.getProjectById(this.sessionId);
    const preferredModel = project?.preferredModel;
    const parsedModel = this.parsePreferredModel(preferredModel);

    const body: MessageBody = {
      messageID,
      parts: [{ id: this.createPartId(), type: 'text', text }],
      agent: DEFAULT_PROMPT_AGENT
    };

    if (parsedModel) {
      body.model = parsedModel;
    }

    this.lastRequestedAgent = body.agent || '(default)';
    this.lastRequestedModel = body.model ? JSON.stringify(body.model) : '(default)';

    console.info(
      '[ChatPage] prompt body prepared:',
      'messageID=', messageID,
      'agent=', body.agent || '(default)',
      'preferredModelRaw=', preferredModel || '(none)',
      'model=', body.model ? JSON.stringify(body.model) : '(default)'
    );

    return body;
  }


  private stopSse() {
    if (this.sseReq) {
      this.sseReq.off('dataReceive');
      this.sseReq.off('dataEnd');
      this.sseReq.destroy();
      this.sseReq = null;
    }
    if (this.sseReconnectTimer !== -1) {
      clearTimeout(this.sseReconnectTimer);
      this.sseReconnectTimer = -1;
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
      this.logAlign('sse.dataEnd');
      this.sseReq = null;
      this.sseBuffer = '';
      // 自动重连
      this.scheduleSseReconnect(3000);
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
          this.logAlign('sse.connect.error');
          this.sseReq = null;
          this.sseBuffer = '';
          this.scheduleSseReconnect(2000);
        } else {
          console.info('[ChatPage][SSE] connected, responseCode:', responseCode);
          this.logAlign('sse.connect.ok', { responseCode });
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
      this.clearPendingState(true);
      this.abortSessionBestEffort('session.error');
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
      if (this.eventFetchTimer !== -1) {
        clearTimeout(this.eventFetchTimer);
        this.eventFetchTimer = -1;
      }
      const statusType = props.status?.type ?? 'idle';
      this.lastStatusType = statusType;
      console.info('[ChatPage][SSE] session.status:', statusType, 'payload=', JSON.stringify(props.status), 'loadingMsgId=', this.loadingMsgId);
      this.logAlign('sse.session.status', { statusType });

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
        if (!this.loadingMsgId) return;
        if (!this.promptAccepted) {
          console.info('[ChatPage][SSE] idle ignored before prompt accepted');
          return;
        }
        const promptMessageId = this.currentPromptMessageId;
        const loadingMessageId = this.loadingMsgId;
        this.logAlign('sse.idle.refreshMessages');
        console.info('[ChatPage][SSE] calling fetchAndRefreshMessages, loadingMsgId=', loadingMessageId);
        this.fetchAndRefreshMessages(promptMessageId, loadingMessageId, 0);
      } else if (statusType === 'busy') {
        this.isLoading = true;
        this.logAlign('sse.busy');
      }
      return;
    }

    if (event.type === 'message.updated') {
      if (!props?.info || props.info.sessionID !== this.realSessionId) return;
      console.info('[SSE] message.updated props.info:', JSON.stringify(props?.info));
      this.upsertMessageInfo(props.info);
      this.rebuildDisplayFromCache();
      this.scrollToBottom();
      return;
    }

    if (event.type === 'message.part.updated') {
      if (!props?.part || props.part.sessionID !== this.realSessionId) return;
      console.info('[SSE] message.part.updated props.part:', JSON.stringify(props?.part));
      this.upsertMessagePart(props.part);
      this.rebuildDisplayFromCache();
      this.scrollToBottom();
      return;
    }

    if (event.type === 'message.part.delta') {
      if (!props?.messageID || !props?.partID || !props?.field || !props?.delta) return;
      console.info('[SSE] message.part.delta field:', props.field, 'delta:', props.delta?.slice(0, 50));
      this.applyMessagePartDelta(props.messageID, props.partID, props.field, props.delta);
      this.rebuildDisplayFromCache();
      this.scrollToBottom();
      return;
    }

    if (event.type === 'message.part.removed') {
      if (!props?.messageID || !props?.partID) return;
      this.removeMessagePart(props.messageID, props.partID);
      this.rebuildDisplayFromCache();
      this.scrollToBottom();
      return;
    }

    if (event.type === 'message.removed') {
      if (!props?.messageID) return;
      this.removeMessageInfo(props.messageID);
      this.rebuildDisplayFromCache();
      this.scrollToBottom();
      return;
    }
  }

  // 拉取消息历史并刷新显示
  private fetchAndRefreshMessages(promptMessageId?: string, loadingMessageId?: string, retryCount: number = 0) {
    const effectivePromptMessageId = promptMessageId || this.currentPromptMessageId || undefined;
    const effectiveLoadingMessageId = loadingMessageId || this.loadingMsgId || undefined;
    console.info('[ChatPage] fetchAndRefreshMessages called, retryCount=', retryCount, 'loadingMsgId=', effectiveLoadingMessageId, 'promptMsgId=', effectivePromptMessageId);
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
          console.info('[ChatPage] fetchAndRefreshMessages got', msgs.length, 'messages from server');
          const promptIndex = effectivePromptMessageId
            ? msgs.findIndex(m => m.info.id === effectivePromptMessageId)
            : -1;
          const assistantAfterPrompt = promptIndex >= 0
            ? msgs.slice(promptIndex + 1).find(m => {
              if (m.info.role !== 'assistant') return false;
              const content = this.viewModel.formatMessage(m);
              return !!content && content.trim().length > 0;
            })
            : undefined;
          const responseModel = assistantAfterPrompt?.info?.model
            ? JSON.stringify(assistantAfterPrompt.info.model)
            : '(none)';

          if (effectivePromptMessageId && promptIndex < 0 && retryCount < 3) {
            console.info('[ChatPage] prompt not persisted yet, retry fetch', 'retry=', retryCount + 1);
            setTimeout(() => {
              this.fetchAndRefreshMessages(effectivePromptMessageId, effectiveLoadingMessageId, retryCount + 1);
            }, 400);
            return;
          }

          if (effectivePromptMessageId && promptIndex < 0) {
            // 如果 prompt 还未落库，不要清 pending；继续等待并保持可见反馈。
            this.isLoading = true;
            this.logAlign('fetch.waitPromptPersisted');
            setTimeout(() => {
              if (!this.promptAccepted) return;
              if (!this.loadingMsgId) return;
              if (effectiveLoadingMessageId && this.loadingMsgId !== effectiveLoadingMessageId) return;
              this.fetchAndRefreshMessages(effectivePromptMessageId, effectiveLoadingMessageId, 0);
            }, 1200);
            this.scrollToBottom();
            return;
          }

          if (effectivePromptMessageId && promptIndex >= 0 && !assistantAfterPrompt && retryCount < 8) {
            console.info('[ChatPage] assistant not ready yet, retry fetch', 'retry=', retryCount + 1);
            setTimeout(() => {
              this.fetchAndRefreshMessages(effectivePromptMessageId, effectiveLoadingMessageId, retryCount + 1);
            }, 400);
            return;
          }

          if (effectivePromptMessageId && promptIndex >= 0 && !assistantAfterPrompt) {
            // 对齐 web 行为：没有 assistant 落库时继续保持等待态，不提前清空 loading。
            this.resetMessageCacheFromServer(msgs);
            this.rebuildDisplayFromCache();
            this.isLoading = true;
            this.logAlign('fetch.waitAssistant');
            setTimeout(() => {
              if (!this.promptAccepted) return;
              if (!this.loadingMsgId) return;
              if (effectiveLoadingMessageId && this.loadingMsgId !== effectiveLoadingMessageId) return;
              this.fetchAndRefreshMessages(effectivePromptMessageId, effectiveLoadingMessageId, 0);
            }, 1200);
            this.scrollToBottom();
            return;
          }

          this.logAlign('fetch.messages', {
            total: msgs.length,
            promptIndex,
            hasAssistantAfterPrompt: !!assistantAfterPrompt,
            responseModel
          });
          const displayMsgs: DisplayMessage[] = msgs.map((msg, i): DisplayMessage => ({
            id: msg.info.id || `msg-${i}`,
            role: msg.info.role as 'user' | 'assistant',
            content: this.viewModel.formatMessage(msg),
            timestamp: msg.info.time.created
          }));

          this.resetMessageCacheFromServer(msgs);

          this.messages = displayMsgs;

          if (effectiveLoadingMessageId) {
            this.messages = this.messages.filter(m => m.id !== effectiveLoadingMessageId);
          }

          this.clearPendingState(false);
          this.lastStatusType = 'idle';
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

        Button(this.isLoading ? '停止' : '发送')
          .margin({ left: 8 })
          .backgroundColor('#07C160')
          .borderRadius(20)
          .enabled(this.isLoading || this.inputText.length > 0)
          .onClick(() => {
            if (this.isLoading) {
              this.stopCurrentRunByUser();
              return;
            }
            this.sendMessage();
          })
      }
      .padding({ left: 12, right: 12, top: 8, bottom: 20 })
      .backgroundColor('#F5F5F5')
    }
    .width('100%')
    .height('100%')
  }
}
