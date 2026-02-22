import React from 'react';
import { Panel, PanelResizeHandle } from 'react-resizable-panels';
import type { Message } from '../types';
import ChatMessage from './ChatMessage';
import LoadingIndicator from './LoadingIndicator';

interface ChatColumnProps {
  messages: Message[];
  loading: boolean;
  input: string;
  onInputChange: (value: string) => void;
  onSend: () => void;
  messagesEndRef: React.RefObject<HTMLDivElement>;
  panelId?: string;
  order?: number;
  defaultSize?: number;
  minSize?: number;
  handleId?: string;
  showHandle?: boolean;
  emptyState?: React.ReactNode;
  renderFooter?: (defaultFooter: React.ReactNode) => React.ReactNode;
}

const ChatColumn: React.FC<ChatColumnProps> = ({
  messages,
  loading,
  input,
  onInputChange,
  onSend,
  messagesEndRef,
  panelId,
  order = 3,
  defaultSize = 30,
  minSize = 20,
  handleId,
  showHandle = true,
  emptyState,
  renderFooter,
}) => {
  const defaultFooter = (
    <div className="workbench-input-area">
      <input
        type="text"
        value={input}
        onChange={(e) => onInputChange(e.target.value)}
        onKeyPress={(e) => e.key === 'Enter' && onSend()}
        placeholder="Ask Agent to do something..."
        disabled={loading}
      />
      <button onClick={onSend} disabled={loading}>
        {loading ? '...' : 'Run'}
      </button>
    </div>
  );

  return (
    <>
      {showHandle && <PanelResizeHandle className="h-resizer" id={handleId} />}
      <Panel id={panelId} order={order} defaultSize={defaultSize} minSize={minSize}>
        <div className="workbench-chat-container">
          <div className="messages-list">
            {messages.length === 0 && emptyState}
            {messages.map((msg, i) => (
              <ChatMessage key={i} msg={msg} />
            ))}
            {loading && <LoadingIndicator />}
            <div ref={messagesEndRef} />
          </div>

          {renderFooter ? renderFooter(defaultFooter) : defaultFooter}
        </div>
      </Panel>
    </>
  );
};

export default ChatColumn;
