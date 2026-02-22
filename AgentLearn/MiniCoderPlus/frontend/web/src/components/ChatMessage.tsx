import React from 'react';
import type { Message } from '../types';

interface ChatMessageProps {
  msg: Message;
}

const ChatMessage: React.FC<ChatMessageProps> = ({ msg }) => {
  if (msg.role === 'tool') return null;
  // 过滤来自 assistant 或 tool call 的空内容消息，避免出现空白气泡
  // 除非该消息携带明确的思维链过程 (isThought)。
  if (!msg.content?.trim() && !msg.isThought) return null;

  return (
    <div className={`message-wrapper ${msg.role} ${msg.isThought ? 'thought' : ''}`}>
      <div className="message-icon">
        {msg.role === 'user' ? '👤' : '🤖'}
      </div>
      <div className="message-content">
        {msg.isThought && <div className="thought-badge">Thought Process</div>}
        <div className="message-text">{msg.content}</div>
      </div>
    </div>
  );
};

export default ChatMessage;
