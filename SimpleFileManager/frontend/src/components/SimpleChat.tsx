import { useState, useRef, useCallback, useEffect } from 'react';
import { Link } from 'react-router-dom';
import { abortAgentChat, streamAgentMessage } from '../api';
import { useChatSessions } from '../hooks/useChatSessions';
import { agentStream, getAgentStreamState, useAgentStream } from '../stores/agentStream';
import type { ToolActivity } from '../stores/agentStream';
import { useProviders } from '../hooks/useProviders';
import { ChatLayout } from './ui/ChatLayout';
import { Icon } from './ui/Icon';
import { useToast } from './ui/Toast';

const EXAMPLE_PROMPTS = [
  '这个目录里有哪些文件？',
  '找出所有名字带 screenshot 的图片',
  '搜索哪些文件里提到了「合同」',
  '按类型帮我整理文件',
];

/**
 * Empty state stays light: greeting plus example prompts. The tool reference
 * lives on its own page — dumping five tool schemas here buried the
 * conversation behind a wall of English.
 */
function ChatWelcome({ onPick }: { onPick: (prompt: string) => void }) {
  return (
    <div className="h-full flex flex-col items-center justify-center text-slate-400 space-y-4 px-4">
      <Icon name="chat" size={40} className="text-slate-300" />
      <div className="text-center">
        <div className="font-medium text-slate-600 mb-1">Agent 对话</div>
        <div className="text-sm">我可以分析文件、搜索内容，并生成待你审批的整理计划</div>
      </div>

      <div className="flex flex-wrap justify-center gap-1.5 max-w-lg">
        {EXAMPLE_PROMPTS.map(p => (
          <button
            key={p}
            onClick={() => onPick(p)}
            className="px-2.5 py-1.5 rounded-lg bg-white border border-slate-200 text-xs text-slate-600 hover:border-indigo-300 hover:text-indigo-600 transition-colors"
          >
            {p}
          </button>
        ))}
      </div>

      <div className="text-xs text-slate-400">
        我只能读取文件，任何改动都会先生成待你审批的计划。
        <Link to="/agent-tools" className="text-indigo-500 hover:underline ml-1">查看 Agent 能力</Link>
      </div>
    </div>
  );
}

export function SimpleChat() {
  const { toast } = useToast();

  const { sessions, currentSessionId, createSession, startNewSession, selectSession, deleteSession, addMessage, reloadSessions } = useChatSessions('agent');
  const providersApi = useProviders();
  // Per-turn provider override. The chat UI shows a dropdown next to
  // the send button; whatever the user picks goes out on the next
  // message as req.provider_id. After the server resolves it, the
  // session's provider_id is updated to the same value, so the next
  // turn defaults to the user's pick even without the override.
  // Switching back to the dropdown's "main" clears the override.
  const [providerId, setProviderId] = useState<string | null>(null);
  // The running turn's state lives in a module-level store, not here:
  // navigating away unmounts this page, and the turn keeps going. Reading
  // it from the store means a fresh mount picks the turn back up and shows
  // the live bubble, the step counter and the stop button.
  const stream = useAgentStream();
  const { isLoading, streamingText, currentStep, livePerf, liveProviderName } = stream;
  const sendRef = useRef<((content: string) => Promise<void>) | null>(null);

  // Is the running turn *this* conversation's turn? Both halves matter:
  // the turn has to belong to the conversation on screen, and there has
  // to be a turn at all.
  //
  // Without the first half, a turn running in one conversation showed
  // its answer in another -- an answer that belongs to the other one,
  // with a stop button that would stop a turn the user cannot see.
  // Without the second half, an empty page (both ids null) looked busy
  // -- which disabled the provider dropdown and showed a running state
  // with nothing running.
  const liveHere = stream.isLoading && stream.sessionId === currentSessionId;
  const toolActivity = liveHere && stream.sessionId ? stream.traces[stream.sessionId] : undefined;

  const updateTrace = useCallback((sessionId: string, update: (prev: ToolActivity[]) => ToolActivity[]) => {
    agentStream.trace(sessionId, update);
  }, []);

  // A reload or a closed tab cuts the stream. The server keeps whatever
  // the model produced, but the user should know before it happens.
  useEffect(() => {
    if (!isLoading) return;
    const handler = (e: BeforeUnloadEvent) => {
      e.preventDefault();
      e.returnValue = '';
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, [isLoading]);

  // A turn can outlive a page navigation: the store keeps it running, and
  // this mount may not be the one that started it. When it ends, the store
  // clears itself but this mount's session list is still stale -- reload so
  // the saved reply shows up without the user having to navigate again.
  const wasLoading = useRef(false);
  useEffect(() => {
    if (wasLoading.current && !isLoading) {
      reloadSessions();
    }
    wasLoading.current = isLoading;
  }, [isLoading, reloadSessions]);

  const handleStop = useCallback(() => {
    const controller = agentStream.getController();
    const { sessionId, streamingText: partial } = getAgentStreamState();
    agentStream.setController(null);
    if (!controller) return;
    // Record the stop right away, with what the user already saw. The
    // server cannot notice a dropped connection promptly (it is waiting
    // on the model), so without this the partial answer would only be
    // saved once the model next spoke.
    if (sessionId) {
      abortAgentChat(sessionId, partial).catch(() => {});
    }
    controller.abort();
  }, []);

  const handleSendMessage = async (content: string) => {
    let sessionId = currentSessionId;
    if (!sessionId) {
      try {
        sessionId = await createSession();
      } catch (e) {
        toast(e instanceof Error ? e.message : '创建会话失败', 'error');
        return;
      }
    }
    await addMessage(sessionId, { role: 'user', content });

    agentStream.start(sessionId, Date.now());
    // a fresh turn starts a fresh trace for that session
    updateTrace(sessionId, () => []);

    // Anchor the live perf header to the moment the user pressed send. The
    // clock is rendered from this immediately, before any token arrives,
    // so the user sees the request start time without waiting on the
    // model. Subsequent fields fill in as events come in.
    const clockMs = Date.now();
    agentStream.perf({ clockMs });

    const controller = new AbortController();
    agentStream.setController(controller);

    let reply = '';
    let streamError: string | null = null;
    let turnPerf: { ttft_ms: number; total_ms: number; tokens: number; tps: number } = {
      ttft_ms: 0, total_ms: 0, tokens: 0, tps: 0,
    };
    // Stash the name on the component so the live perf header can
    // render "via <name>" as soon as the SSE `done` lands, before
    // the persisted bubble takes over. Cleared on turn end.
    agentStream.providerName(undefined);
    // Client-side anchors for the live header. Used only when the server
    // doesn't emit the perf fields (e.g. legacy build) — and to measure
    // TTFT for the streaming bubble the moment the first text delta
    // lands (reasoning chunk or answer chunk — whichever arrives first),
    // before the server's "done" event arrives. Mirrors the backend's
    // "first LLM text byte" semantics so the streaming bubble's TTFT
    // agrees with the persisted message's TTFT.
    const clientT0 = performance.now();
    let clientFirstTokenAt: number | null = null;

    try {
      await streamAgentMessage(
        content,
        sessionId,
        (event) => {
          if (event.type === 'reasoning') {
            // One ordered stream, not two: the model reasons, calls a tool,
            // reasons again, calls another tool. Merging them per-arrival is
            // what makes the transcript read like what actually happened
            // instead of "all thinking, then all tool calls, then the answer".
            // Consecutive reasoning deltas fold into the previous block so a
            // step does not fragment into dozens of one-word entries.
            updateTrace(sessionId, (prev) => {
              const last = prev[prev.length - 1];
              if (last?.kind === 'reasoning') {
                return [...prev.slice(0, -1), { ...last, text: last.text + (event.text ?? '') }];
              }
              return [...prev, { kind: 'reasoning', text: event.text ?? '' }];
            });
          } else if (event.type === 'token') {
            if (!event.text) return;
            reply += event.text;
            agentStream.token(event.text);
            // First token: stamp TTFT and reveal it in the live header.
            // The server's measurement is more accurate and overwrites
            // this on the "done" event, but the client value lets the
            // streaming bubble show TTFT the moment the first character
            // lands, without waiting for the whole turn to finish.
            if (clientFirstTokenAt === null) {
              clientFirstTokenAt = performance.now();
              const ttft = Math.round(clientFirstTokenAt - clientT0);
              agentStream.perf({ ttft_ms: ttft });
            }
          } else if (event.type === 'tool_start') {
            updateTrace(sessionId, prev => [...prev, { kind: 'tool', tool: event.tool, status: 'running', args: event.args }]);
          } else if (event.type === 'tool') {
            updateTrace(sessionId, (prev) => {
              // findLastIndex needs ES2023; reverse scan keeps the compiler
              // target unchanged. Matching the *last* running entry matters now
              // that reasoning entries are interleaved with tool calls.
              let running = -1;
              for (let i = prev.length - 1; i >= 0; i--) {
                if (prev[i].status === 'running') { running = i; break; }
              }
              if (running === -1) {
                return [...prev, { kind: 'tool', tool: event.tool, status: 'done', args: event.args, result: event.result }];
              }
              const next = prev.slice();
              next[running] = {
                ...next[running],
                status: 'done',
                args: event.args ?? next[running].args,
                result: event.result,
              };
              return next;
            });
          } else if (event.type === 'step') {
            agentStream.step(event.step);
          } else if (event.type === 'done') {
            reply = event.response || reply;
            // Server echoes the resolved provider. Capture for the
            // persisted bubble and for the session-row update below.
            // The live badge wants the model name (not the operator-
            // given handle), so look it up in the providers list —
            // if the providers are still loading the user just sees
            // the model field on the next turn.
            if (event.provider_name) {
              const match = providersApi.providers.find(p => p.id === event.provider_id);
              agentStream.providerName(match?.model || event.provider_name);
            }
            // The server emits the authoritative perf numbers in the
            // "done" event. Mirror them into both turnPerf (used when
            // persisting the assistant message) and livePerf (kept
            // visible until the persisted bubble takes its place).
            if (typeof event.ttft_ms === 'number') {
              turnPerf = {
                ttft_ms: event.ttft_ms,
                total_ms: event.total_ms ?? 0,
                tokens: event.tokens ?? 0,
                tps: event.tps ?? 0,
              };
              agentStream.perf({
                ttft_ms: turnPerf.ttft_ms,
                total_ms: turnPerf.total_ms,
                tokens: turnPerf.tokens,
                tps: turnPerf.tps,
              });
            }
            // Covers the case where the stream ends without the client having
            // seen every reasoning delta.
          } else if (event.type === 'error') {
            streamError = event.message;
          }
        },
        controller.signal,
        { providerId: providerId }
      );
    } catch (e) {
      // An abort is a deliberate stop, not a failure — keep what arrived.
      if (!controller.signal.aborted) {
        streamError = e instanceof Error ? e.message : '请求失败';
      }
    } finally {
      agentStream.setController(null);
      agentStream.cancelTools(sessionId);
      agentStream.end();
    }

    // The server writes the assistant reply to the transcript itself.
    // It has to: a reload, a closed tab or a lost connection used to
    // throw the answer away even though the model had produced it.
    // All that is left here is to pick the saved message up.
    //
    // A stopped turn needs a moment -- the server notices the dropped
    // connection and writes the partial reply before we can reload.
    await new Promise(r => setTimeout(r, controller.signal.aborted ? 800 : 150));
    await reloadSessions();

    if (streamError && !reply.trim()) {
      toast(streamError, 'error');
    }
    // Drop the live header once the persisted bubble (which carries the
    // same numbers via the message's perf fields) takes its place.
    agentStream.end();
    // Refresh sessions so the new assistant message appears in the
    // sidebar with the resolved provider_id — without this the row
    // would still show the previous (pre-override) state until the
    // user clicks away and back.
    await providersApi.refresh();
  };

  // keeps the ref pointing at the latest closure without re-rendering
  sendRef.current = handleSendMessage;

  // "New conversation" wrapper. The new session's default provider is
  // already set to the main on the server; sync the dropdown to match
  // so the user sees what the session will actually use.
  const handleNewSession = () => {
    startNewSession();
    setProviderId(null);
  };

  // When the user clicks an existing session, sync the dropdown to
  // whatever the session was last used with. Sessions created before
  // the providers refactor have provider_id=null, in which case we
  // show the dropdown's neutral "main" placeholder.
  useEffect(() => {
    if (!currentSessionId) {
      setProviderId(null);
      return;
    }
    const session = sessions.find(s => s.id === currentSessionId);
    setProviderId(session?.provider_id ?? null);
  }, [currentSessionId, sessions]);

  // Snap to the main provider once on first load so the empty state
  // shows the actual model name. A deliberate user choice (null) is
  // never overridden.
  useEffect(() => {
    if (providerId === null && providersApi.main && !currentSessionId) {
      setProviderId(providersApi.main.id);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="h-full">
      <ChatLayout
        sessions={sessions}
        currentSessionId={currentSessionId}
        onSelectSession={selectSession}
        onNewSession={handleNewSession}
        onDeleteSession={deleteSession}
        onSendMessage={handleSendMessage}
        isLoading={liveHere}
        streamingText={liveHere ? streamingText : ''}
        toolActivity={toolActivity}
        currentStep={currentStep}
        livePerf={livePerf ?? undefined}
        liveProviderName={liveProviderName}
        providers={providersApi.providers}
        providerId={providerId}
        onProviderChange={setProviderId}
        onStop={handleStop}
        emptyState={<ChatWelcome onPick={(p) => sendRef.current?.(p)} />}
      />
    </div>
  );
}
