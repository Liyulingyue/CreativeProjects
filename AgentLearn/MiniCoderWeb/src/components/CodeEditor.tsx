import React, { useState } from 'react';
import './CodeEditor.css';

interface CodeEditorProps {
  code: string;
  onCodeChange: (code: string) => void;
  placeholder?: string;
  readOnly?: boolean;
}

export const CodeEditor: React.FC<CodeEditorProps> = ({
  code,
  onCodeChange,
  placeholder = '请输入代码...',
  readOnly = false
}) => {
  return (
    <div className="code-editor">
      <textarea
        className={`code-textarea ${readOnly ? 'readonly' : ''}`}
        value={code}
        onChange={(e) => onCodeChange(e.target.value)}
        placeholder={placeholder}
        readOnly={readOnly}
        spellCheck={false}
      />
    </div>
  );
};