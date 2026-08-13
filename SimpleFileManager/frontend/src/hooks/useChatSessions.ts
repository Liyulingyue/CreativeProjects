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

  const selectSession = useCallback((id: string) => {
    setCurrentSessionId(id);
    setSessions(prev => prev.map(s =>
      s.id === id ? { ...s, updated_at: Date.now() } : s
    ));
  }, []);

  const deleteSession = useCallback(async (id: string) => {
    await deleteChatSession(id);
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
      message.sources
    );
    setSessions(prev => prev.map(s => {
      if (s.id !== sessionId) return s;
      return {
        ...s,
        messages: [...s.messages, newMessage],
        updated_at: Date.now(),
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
    selectSession,
    deleteSession,
    updateSessionTitle,
    addMessage,
    getCurrentSession,
  };
}
