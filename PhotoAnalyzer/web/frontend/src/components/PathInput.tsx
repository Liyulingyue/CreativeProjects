import { useState, useEffect, useRef, useCallback } from "react";
import { suggestPath } from "@/api/fs";
import type { FsEntry } from "@/api/types";

interface PathInputProps {
  value: string;
  onChange: (value: string) => void;
  onSelect: (path: string) => void;
  onBrowse?: () => void;
  placeholder?: string;
}

export function PathInput({ value, onChange, onSelect, onBrowse, placeholder }: PathInputProps) {
  const [suggestions, setSuggestions] = useState<FsEntry[]>([]);
  const [open, setOpen] = useState(false);
  const [highlightIndex, setHighlightIndex] = useState(-1);
  const containerRef = useRef<HTMLDivElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const fetchSuggestions = useCallback(async (q: string) => {
    try {
      const result = await suggestPath(q);
      setSuggestions(result.suggestions);
      setOpen(result.suggestions.length > 0);
      setHighlightIndex(-1);
    } catch {
      setSuggestions([]);
      setOpen(false);
    }
  }, []);

  const handleChange = (v: string) => {
    onChange(v);
    clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => fetchSuggestions(v), 200);
  };

  const handleSelect = (entry: FsEntry) => {
    const newPath = entry.path + "/";
    onChange(newPath);
    setOpen(false);
    setSuggestions([]);
    fetchSuggestions(newPath);
  };

  const handleConfirm = () => {
    if (value.trim()) {
      onSelect(value.trim());
      setOpen(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!open || suggestions.length === 0) {
      if (e.key === "Enter") handleConfirm();
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlightIndex((i) => Math.min(i + 1, suggestions.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlightIndex((i) => Math.max(i - 1, -1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (highlightIndex >= 0) {
        handleSelect(suggestions[highlightIndex]);
      } else {
        handleConfirm();
      }
    } else if (e.key === "Escape") {
      setOpen(false);
    }
  };

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  return (
    <div className="path-input" ref={containerRef}>
      <div className="path-input__field">
        <input
          type="text"
          value={value}
          onChange={(e) => handleChange(e.target.value)}
          onKeyDown={handleKeyDown}
          onFocus={() => {
            if (suggestions.length > 0) setOpen(true);
            else fetchSuggestions(value);
          }}
          placeholder={placeholder || "输入路径，如 /home 或 /mnt/nas"}
          autoComplete="off"
          spellCheck={false}
        />
        <button className="btn btn--sm btn--primary" onClick={handleConfirm} disabled={!value.trim()}>
          分析
        </button>
        {onBrowse && (
          <button className="btn btn--sm" onClick={onBrowse}>
            浏览
          </button>
        )}
      </div>

      {open && suggestions.length > 0 && (
        <div className="path-input__dropdown">
          {suggestions.map((entry, i) => (
            <div
              key={entry.path}
              className={`path-input__option ${i === highlightIndex ? "path-input__option--highlight" : ""}`}
              onMouseDown={(e) => { e.preventDefault(); handleSelect(entry); }}
              onMouseEnter={() => setHighlightIndex(i)}
            >
              <span className="path-input__option-icon">📁</span>
              <span className="path-input__option-name">{entry.name}</span>
              <span className="path-input__option-path">{entry.path}</span>
              {entry.children_count !== null && entry.children_count !== undefined && (
                <span className="path-input__option-count">{entry.children_count}</span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
