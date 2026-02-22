import React, { useState } from 'react';
import type { Message } from '../types';

interface ChatMessageProps {
  msg: Message;
  sessionId?: string;
  onFeedbackChange?: (messageId: number, feedback: string) => void;
}

const THUMBS_UP = '👍';
const THUMBS_DOWN = '👎';
const USER_ICON = '👤';
const ROBOT_ICON = '🤖';

const ChatMessage: React.FC<ChatMessageProps> = ({ msg, sessionId, onFeedbackChange }) => {
  const [givenFeedback, setGivenFeedback] = useState<string | undefined>(msg.feedback);
  const [showCommentInput, setShowCommentInput] = useState(false);
  const [pendingFeedback, setPendingFeedback] = useState<string | null>(null);
  const [commentText, setCommentText] = useState(msg.feedback_comment || '');

  // Sync state with props when msg changes (e.g. from history load)
  React.useEffect(() => {
    setGivenFeedback(msg.feedback);
  }, [msg.feedback]);

  React.useEffect(() => {
    if (msg.feedback_comment) {
      setCommentText(msg.feedback_comment);
    }
  }, [msg.feedback_comment]);

  if (msg.role === 'tool') return null;
  
  // Show message if it has content, or if it's a thought/tool call
  const hasContent = msg.content?.trim();
  const hasToolCalls = msg.tool_calls && Object.keys(msg.tool_calls).length > 0;
  const isThought = msg.isThought || hasToolCalls;
  
  if (!hasContent && !isThought) return null;

  // Debug: log when we should show feedback buttons
  if (msg.role === 'assistant') {
    console.log('[ChatMessage] Assistant message:', {
      id: msg.id,
      hasId: !!msg.id,
      sessionId,
      hasSessionId: !!sessionId,
      shouldShowFeedback: !!msg.id && !!sessionId,
      content: msg.content?.substring(0, 50)
    });
  }

  const handleFeedback = async (feedback: string) => {
    console.log('[handleFeedback] Called with:', { msg_id: msg.id, sessionId, feedback });
    
    if (!msg.id || !sessionId) {
      console.error('[handleFeedback] Missing required params:', { id: msg.id, sessionId });
      return;
    }

    // Allow changing feedback - always show comment input dialog
    setPendingFeedback(feedback);
    setShowCommentInput(true);
  };

  const submitFeedbackWithComment = async () => {
    if (!msg.id || !sessionId || !pendingFeedback) return;

    const comment = commentText.trim() || undefined;
    setGivenFeedback(pendingFeedback);
    onFeedbackChange?.(msg.id, pendingFeedback);

    // Send feedback to backend for training data collection
    const commentParam = comment ? `&comment=${encodeURIComponent(comment)}` : '';
    const feedbackUrl = `/api/v1/chat/feedback?message_id=${msg.id}&session_id=${encodeURIComponent(sessionId)}&feedback=${encodeURIComponent(pendingFeedback)}${commentParam}`;
    console.log('[handleFeedback] Sending POST to:', feedbackUrl);
    
    try {
      const resp = await fetch(feedbackUrl, { method: 'POST' });
      console.log('[handleFeedback] Response status:', resp.status);
      
      if (!resp.ok) {
        const errText = await resp.text();
        console.error('[handleFeedback] Failed to save feedback. Status:', resp.status, 'Body:', errText);
        setGivenFeedback(undefined);
      } else {
        const result = await resp.json();
        console.log('[handleFeedback] Success:', result);
      }
    } catch (e) {
      console.error('[handleFeedback] Error:', e);
      setGivenFeedback(undefined);
    } finally {
      setShowCommentInput(false);
      setPendingFeedback(null);
    }
  };

  return (
    <>
      <div className={`message-wrapper ${msg.role} ${isThought ? 'thought' : ''}`}>
        <div className="message-icon">
          {msg.role === 'user' ? USER_ICON : ROBOT_ICON}
        </div>
        <div className="message-content">
          {isThought && <div className="thought-badge">Thought Process</div>}
          {hasContent && <div className="message-text">{msg.content}</div>}
          
          {/* Feedback buttons for assistant messages (for training data) */}
          {msg.role === 'assistant' && msg.id && sessionId && (
            <div className="message-feedback" style={{
              display: 'flex',
              gap: '8px',
              marginTop: '8px',
              alignItems: 'flex-start',
              fontSize: '12px',
              color: '#6b7280',
              flexWrap: 'wrap'
            }}>
              <span style={{ whiteSpace: 'nowrap' }}>评分:</span>
              <button
                onClick={() => handleFeedback(THUMBS_UP)}
                disabled={false}
                title="好评 - 帮助改进模型"
                style={{
                  background: givenFeedback === THUMBS_UP ? '#d1fae5' : 'transparent',
                  border: givenFeedback === THUMBS_UP ? '1px solid #6ee7b7' : '1px solid #e5e7eb',
                  borderRadius: '4px',
                  padding: '4px 8px',
                  cursor: 'pointer',
                  fontSize: '14px',
                  transition: 'all 0.2s',
                  opacity: givenFeedback && givenFeedback !== THUMBS_UP ? 0.5 : 1
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = '#f0fdf4';
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = givenFeedback === THUMBS_UP ? '#d1fae5' : 'transparent';
                }}
              >
                {THUMBS_UP} 好
              </button>
              <button
                onClick={() => handleFeedback(THUMBS_DOWN)}
                disabled={false}
                title="差评 - 帮助改进模型"
                style={{
                  background: givenFeedback === THUMBS_DOWN ? '#fee2e2' : 'transparent',
                  border: givenFeedback === THUMBS_DOWN ? '1px solid #fca5a5' : '1px solid #e5e7eb',
                  borderRadius: '4px',
                  padding: '4px 8px',
                  cursor: 'pointer',
                  fontSize: '14px',
                  transition: 'all 0.2s',
                  opacity: 1
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = '#fef2f2';
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = givenFeedback === THUMBS_DOWN ? '#fee2e2' : 'transparent';
                }}
              >
                {THUMBS_DOWN} 差
              </button>
              
              {/* Display existing comment if feedback was given */}
              {givenFeedback && commentText && (
                <div style={{
                  padding: '2px 8px',
                  background: '#f9fafb',
                  border: '1px solid #e5e7eb',
                  borderRadius: '4px',
                  fontSize: '13px',
                  color: '#4b5563',
                  maxWidth: '400px',
                  wordBreak: 'break-word',
                  fontStyle: 'italic'
                }}>
                  "{commentText}"
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      {/* Comment input modal */}
      {showCommentInput && pendingFeedback && (
        <div style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000
          }}>
            <div style={{
              background: 'white',
              borderRadius: '8px',
              padding: '20px',
              maxWidth: '400px',
              width: '90%',
              boxShadow: '0 20px 25px rgba(0,0,0,0.15)'
            }}>
              <h3 style={{ marginTop: 0, fontSize: '16px', color: '#1f2937' }}>
                {givenFeedback && givenFeedback !== pendingFeedback ? '修改评分' : '添加评论'} {pendingFeedback}
              </h3>
              <p style={{ fontSize: '13px', color: '#6b7280', marginBottom: '12px' }}>
                {givenFeedback && givenFeedback !== pendingFeedback ? '您正在修改评分。' : ''}请输入您对此回复的评论（可选）
              </p>
              <textarea
                value={commentText}
                onChange={(e) => setCommentText(e.target.value)}
                placeholder="请描述您的想法..."
                style={{
                  width: '100%',
                  minHeight: '100px',
                  padding: '8px',
                  border: '1px solid #d1d5db',
                  borderRadius: '4px',
                  fontSize: '13px',
                  fontFamily: 'inherit',
                  boxSizing: 'border-box',
                  resize: 'vertical'
                }}
              />
              <div style={{
                display: 'flex',
                gap: '8px',
                marginTop: '12px',
                justifyContent: 'flex-end'
              }}>
                <button
                  onClick={() => {
                    setShowCommentInput(false);
                    setCommentText('');
                    setPendingFeedback(null);
                  }}
                  style={{
                    padding: '6px 12px',
                    background: '#f3f4f6',
                    border: '1px solid #d1d5db',
                    borderRadius: '4px',
                    cursor: 'pointer',
                    fontSize: '13px',
                    transition: 'all 0.2s'
                  }}
                  onMouseEnter={(e) => e.currentTarget.style.background = '#e5e7eb'}
                  onMouseLeave={(e) => e.currentTarget.style.background = '#f3f4f6'}
                >
                  取消
                </button>
                <button
                  onClick={submitFeedbackWithComment}
                  style={{
                    padding: '6px 12px',
                    background: '#3b82f6',
                    color: 'white',
                    border: 'none',
                    borderRadius: '4px',
                    cursor: 'pointer',
                    fontSize: '13px',
                    fontWeight: '500',
                    transition: 'all 0.2s'
                  }}
                  onMouseEnter={(e) => e.currentTarget.style.background = '#2563eb'}
                  onMouseLeave={(e) => e.currentTarget.style.background = '#3b82f6'}
                >
                  提交
                </button>
              </div>
            </div>
          </div>
      )}
    </>
  );
};

export default ChatMessage;
