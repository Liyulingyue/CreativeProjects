import http from '@ohos.net.http';
import { OpenCodeCore } from '../core/OpenCodeCore';
import { OpenCodeMessage, OpenCodeMessagePart } from '../core/OpenCodeApiClient';

export interface DisplayMessage {
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

export class ChatViewModel {
  private core: OpenCodeCore = OpenCodeCore.getInstance();
  private currentRequest: http.HttpRequest | null = null;

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

  cancelRequest(): void {
    if (this.currentRequest) {
      this.currentRequest.destroy();
      this.currentRequest = null;
    }
  }

  getHeaders(backendUrl: string, authToken: string, directory: string, username?: string): Record<string, string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'x-opencode-directory': encodeURIComponent(directory)
    };
    if (authToken) {
      const authUsername = username || 'opencode';
      headers['Authorization'] = 'Basic ' + this.base64Encode(authUsername + ':' + authToken);
    }
    return headers;
  }

  buildUrlWithDir(baseUrl: string, path: string, directory: string): string {
    const base = baseUrl.replace(/\/+$/, '');
    return `${base}${path}`;
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

  formatMessage(msg: OpenCodeMessage): string {
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

  async loadHistory(
    backendUrl: string,
    authToken: string,
    directory: string,
    realSessionId: string,
    onResult: (messages: DisplayMessage[]) => void
  ): Promise<void> {
    if (!backendUrl) return;

    if (!realSessionId) {
      console.info('[ChatViewModel] No realSessionId yet, skipping history load');
      onResult([]);
      return;
    }

    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = this.buildUrlWithDir(backendUrl, `/session/${encodeURIComponent(realSessionId)}/message`, directory);

    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.GET,
            header: this.getHeaders(backendUrl, authToken, directory),
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
        const displayMessages: DisplayMessage[] = messages.map((msg, index): DisplayMessage => ({
          id: msg.info.id || `msg-${index}`,
          role: msg.info.role as 'user' | 'assistant',
          content: this.formatMessage(msg),
          timestamp: msg.info.time.created
        }));
        onResult(displayMessages);
      } else {
        onResult([]);
      }
    } catch (e) {
      console.error('[ChatViewModel] Load history error:', e);
      onResult([]);
    } finally {
      this.cancelRequest();
    }
  }

  async sendMessage(
    backendUrl: string,
    authToken: string,
    directory: string,
    realSessionId: string,
    sessionId: string,
    sessionName: string,
    text: string,
    onUpdate: (messages: DisplayMessage[]) => void
  ): Promise<string> {
    const loadingId = `loading-${Date.now()}`;

    let currentSessionId = realSessionId;

    if (!currentSessionId) {
      try {
        const resp = await this.core.createBackendSession(backendUrl, authToken, directory, sessionName);
        if (resp && resp.id) {
          currentSessionId = resp.id;
          this.core.setCurrentSession(resp.id);
          await this.core.updateRemoteSessionId(sessionId, resp.id);
          console.info('[ChatViewModel] Backend session created and linked:', currentSessionId);
        } else {
          throw new Error('Failed to create backend session');
        }
      } catch (e) {
        const errorMsg: DisplayMessage = {
          id: `err-${Date.now()}`,
          role: 'assistant',
          content: `创建后端会话失败: ${e.message || e}`,
          timestamp: Date.now()
        };
        onUpdate([errorMsg]);
        return '';
      }
    }

    return currentSessionId;
  }

  async performSendMessage(
    backendUrl: string,
    authToken: string,
    directory: string,
    realSessionId: string,
    text: string,
    loadingId: string,
    onUpdate: (messages: DisplayMessage[]) => void,
    onError: (msg: string) => void
  ): Promise<void> {
    this.cancelRequest();
    this.currentRequest = http.createHttp();
    const url = this.buildUrlWithDir(backendUrl, `/session/${encodeURIComponent(realSessionId)}/message`, directory);

    const body: MessageBody = {
      parts: [{ type: 'text', text: text }]
    };

    try {
      const result = await new Promise<http.HttpResponse>((resolve, reject) => {
        this.currentRequest!.request(
          url,
          {
            method: http.RequestMethod.POST,
            header: this.getHeaders(backendUrl, authToken, directory),
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

      if (result.responseCode === 200) {
        const response = JSON.parse(result.result as string) as OpenCodeMessage;
        const content = this.formatMessage(response);
        const assistantMsg: DisplayMessage = {
          id: response.info.id || `resp-${Date.now()}`,
          role: 'assistant',
          content: content || '完成',
          timestamp: Date.now()
        };
        return assistantMsg as unknown as void;
      } else {
        onError(`请求失败: HTTP ${result.responseCode}`);
      }
    } catch (e) {
      onError(`错误: ${e}`);
    } finally {
      this.cancelRequest();
    }
  }
}
