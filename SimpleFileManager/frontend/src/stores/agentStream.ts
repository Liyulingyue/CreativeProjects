import { useSyncExternalStore } from 'react';

/**
 * The state of the turn that is running right now.
 *
 * It lives outside the chat page because the chat page unmounts when
 * the user navigates somewhere else -- and the turn does not stop just
 * because nobody is watching. If this state lived in the component,
 * coming back mid-answer would show a finished-looking screen while
 * the model was still writing: no bubble, no stop button, an input
 * that accepted a second question.
 *
 * The store is module-level, so a fresh mount picks the turn back up
 * and renders it as the live thing it still is.
 */

export interface ToolActivity {
  kind: 'reasoning' | 'tool';
  tool?: string;
  status?: 'running' | 'done' | 'cancelled';
  args?: unknown;
  result?: unknown;
  text?: string;
}

export interface LivePerf {
  clockMs: number;
  ttft_ms?: number;
  total_ms?: number;
  tokens?: number;
  tps?: number;
}

export interface AgentStreamState {
  /** The conversation the running turn belongs to. */
  sessionId: string | null;
  isLoading: boolean;
  streamingText: string;
  currentStep: number | undefined;
  livePerf: LivePerf | null;
  liveProviderName: string | undefined;
  /** Per-session reasoning/tool transcript. */
  traces: Record<string, ToolActivity[]>;
}

let state: AgentStreamState = {
  sessionId: null,
  isLoading: false,
  streamingText: '',
  currentStep: undefined,
  livePerf: null,
  liveProviderName: undefined,
  traces: {},
};

/**
 * The abort controller of the running turn. It has to outlive the
 * component too: the stop button on a freshly mounted chat page has to
 * be able to stop a turn that an earlier mount started.
 */
let controller: AbortController | null = null;

const listeners = new Set<() => void>();

function emit() {
  listeners.forEach(l => l());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

function getSnapshot() {
  return state;
}

function set(patch: Partial<AgentStreamState>) {
  state = { ...state, ...patch };
  emit();
}

export function useAgentStream(): AgentStreamState {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** Read the state without subscribing (for event handlers). */
export function getAgentStreamState(): AgentStreamState {
  return state;
}

export const agentStream = {
  /** A turn has started. `clockMs` anchors the live header's timer. */
  start(sessionId: string, clockMs: number) {
    set({
      sessionId,
      isLoading: true,
      streamingText: '',
      currentStep: undefined,
      livePerf: { clockMs },
      liveProviderName: undefined,
      traces: { ...state.traces, [sessionId]: [] },
    });
  },

  setController(c: AbortController | null) {
    controller = c;
  },

  getController() {
    return controller;
  },

  token(text: string) {
    set({ streamingText: state.streamingText + text });
  },

  step(n: number) {
    set({ currentStep: n });
  },

  perf(patch: Partial<LivePerf>) {
    set({ livePerf: { ...(state.livePerf ?? { clockMs: Date.now() }), ...patch } });
  },

  providerName(name: string | undefined) {
    set({ liveProviderName: name });
  },

  trace(sessionId: string, update: (prev: ToolActivity[]) => ToolActivity[]) {
    const next = update(state.traces[sessionId] || []);
    set({ traces: { ...state.traces, [sessionId]: next } });
  },

  /** A stopped turn leaves its running tools marked as cancelled. */
  cancelTools(sessionId: string) {
    const prev = state.traces[sessionId] || [];
    set({
      traces: {
        ...state.traces,
        [sessionId]: prev.map(t => (t.status === 'running' ? { ...t, status: 'cancelled' as const } : t)),
      },
    });
  },

  end() {
    set({
      isLoading: false,
      streamingText: '',
      currentStep: undefined,
      livePerf: null,
      liveProviderName: undefined,
    });
  },
};
