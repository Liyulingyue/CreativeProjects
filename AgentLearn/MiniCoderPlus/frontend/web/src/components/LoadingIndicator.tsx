import React from 'react';

const LoadingIndicator: React.FC = () => {
  return (
    <div className="message-wrapper assistant loading">
      <div className="message-icon">🤖</div>
      <div className="message-content">
        <div className="typing-indicator">
          <span></span><span></span><span></span>
        </div>
      </div>
    </div>
  );
};

export default LoadingIndicator;
