import { memo } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

/**
 * Renders model output as markdown, without ever mangling raw content.
 *
 * The distinction matters: LLM replies legitimately contain `**`, `##` and
 * `- `, but a text file preview or a JSON tool result that happens to contain
 * a stray asterisk must be shown verbatim -- rendering it would silently
 * change what the file actually says.
 *
 * `force` pins the decision for call sites that know the source: `model` for
 * generated prose, `plain` for anything read back from disk or a tool payload.
 */
export type TextSource = 'auto' | 'model' | 'plain';

/**
 * Models wrap their answer in a ```markdown fence when a prompt merely says
 * "use markdown format". Rendering that verbatim shows a code block containing
 * markdown source, which is the bug this unwraps.
 */
function stripFence(text: string): string {
  const m = text.match(/^\s*```(?:markdown|md)?[ \t]*\r?\n([\s\S]*?)\r?\n?```\s*$/i);
  return m ? m[1] : text;
}

/** Heuristic for `auto`: does this look like markdown rather than plain prose? */
export function looksLikeMarkdown(text: string): boolean {
  const t = stripFence(text);
  if (/^\s*```/.test(t)) return true;
  return [
    /^#{1,6}\s+\S/m, // heading
    /^\s*[-*+]\s+\S/m, // bullet
    /^\s*\d+\.\s+\S/m, // ordered list
    /^\s*>\s+\S/m, // blockquote
    /^\s*\|.+\|\s*$/m, // table
    /\*\*.+?\*\*/s, // bold
    /`[^`\n]+`/, // inline code
    /\[[^\]]+\]\([^)]+\)/, // link
  ].some((re) => re.test(t));
}

export interface MarkdownTextProps {
  children: string;
  source?: TextSource;
  className?: string;
}

function MarkdownTextImpl({ children, source = 'auto', className = '' }: MarkdownTextProps) {
  const useMarkdown = source === 'model' || (source === 'auto' && looksLikeMarkdown(children));

  if (!useMarkdown) {
    return <div className={`whitespace-pre-wrap break-words ${className}`}>{children}</div>;
  }

  return (
    <div className={`markdown-body break-words ${className}`}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          // Links must not navigate the app away, and user content is
          // untrusted: no raw HTML, no javascript: targets.
          a: ({ href, children: kids }) => (
            <a href={href} target="_blank" rel="noopener noreferrer">
              {kids}
            </a>
          ),
        }}
      >
        {stripFence(children)}
      </ReactMarkdown>
    </div>
  );
}

export const MarkdownText = memo(MarkdownTextImpl);
