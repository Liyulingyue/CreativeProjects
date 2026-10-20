import { useState, useEffect, useCallback } from 'react';
import {
  fetchChatSessions,
  createChatSession,
  deleteChatSession,
  addChatMessage,
  updateChatSessionTitle,
  type ChatSession,
  type ChatMessage,
} from '../api';
import { authFetch } from '../auth';

export function useChatSessions(sessionType: string = 'chat') {
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const loadSessions = useCallback(async () => {
    try {
      const data = await fetchChatSessions(sessionType);
      setSessions(data.sessions);
      if (data.current_session_id && !currentSessionId) {
        setCurrentSessionId(data.current_session_id);
      }
    } catch (e) {
      console.error('Failed to load chat sessions:', e);
    } finally {
      setIsLoading(false);
    }
  }, [sessionType]);

  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  const createSession = useCallback(async (): Promise<string> => {
    const session = await createChatSession(sessionType);
    setSessions(prev => [session, ...prev]);
    setCurrentSessionId(session.id);
    return session.id;
  }, [sessionType]);

  // "New conversation" handler. The naive flow — POST /sessions the moment
  // the user clicks the plus button — leaves an empty row in the DB for
  // every abandoned draft and pollutes the sidebar with blanks. The chat
  // send path already creates the session on the first message
  // (`handleSendMessage` checks `if (!sessionId) sessionId = await
  // createSession()`), so all "new" needs to do is clear the current
  // selection. The empty state renders, the input keeps whatever the
  // user has typed, and the DB row materialises on first send.
  const startNewSession = useCallback(() => {
    setCurrentSessionId(null);
  }, []);

  const selectSession = useCallback((id: string) => {
    setCurrentSessionId(id);
    setSessions(prev => prev.map(s =>
      s.id === id ? { ...s, updated_at: Date.now() } : s
    ));
  }, []);

  const deleteSession = useCallback(async (id: string) => {
    await Promise.all([
      deleteChatSession(id),
      // also drop the agent's in-memory conversation context, otherwise it leaks
      authFetch(`/api/agent/sessions/${id}`, { method: 'DELETE' }).catch(() => {}),
    ]);
    setSessions(prev => {
      const filtered = prev.filter(s => s.id !== id);
      if (currentSessionId === id) {
        setCurrentSessionId(filtered.length > 0 ? filtered[0].id : null);
      }
      return filtered;
    });
  }, [currentSessionId]);

  const updateSessionTitle = useCallback(async (id: string, title: string) => {
    await updateChatSessionTitle(id, title);
    setSessions(prev => prev.map(s =>
      s.id === id ? { ...s, title } : s
    ));
  }, []);

const addMessage = useCallback(async (
    sessionId: string,
    message: Omit<ChatMessage, 'id' | 'timestamp'>
  ): Promise<string> => {
    const newMessage = await addChatMessage(
      sessionId,
      message.role,
      message.content,
      message.sources,
      message.reasoning,
      message.activity,
      {
        ttft_ms: message.ttft_ms,
        total_ms: message.total_ms,
        tokens: message.tokens,
        tps: message.tps,
      },
      message.provider_id,
      message.plans
    );
    setSessions(prev => prev.map(s => {
      if (s.id !== sessionId) return s;
      // An assistant message's provider_id is the model that
      // actually served the reply — mirror it onto the session
      // row so the chat UI's "next turn uses the same model"
      // behaviour stays consistent. The server already does the
      // same write to chat_sessions.provider_id, but without
      // this the local cache would keep the stale value (the
      // pre-pick main) and a subsequent useEffect in SimpleChat
      // would snap the dropdown back to main.
      const nextProviderId = newMessage.provider_id ?? s.provider_id;
      return {
        ...s,
        messages: [...s.messages, newMessage],
        updated_at: Date.now(),
        provider_id: nextProviderId,
      };
    }));
    return newMessage.id;
  }, []);

  const getCurrentSession = useCallback(() => {
    return sessions.find(s => s.id === currentSessionId) || null;
  }, [sessions, currentSessionId]);

  return {
    sessions,
    currentSessionId,
    isLoading,
    createSession,
    startNewSession,
    selectSession,
    deleteSession,
    updateSessionTitle,
    addMessage,
    getCurrentSession,
  };
}
