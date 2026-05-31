
export enum MessageSegmentType {
  TEXT,
  THOUGHT,
  STEP
}

export interface MessageSegment {
  type: MessageSegmentType;
  content: string;
  title?: string;
  isCollapsed?: boolean;
}

export enum MarkdownType {
  BOLD,
  CODE,
  CODE_BLOCK,
  TEXT
}

export interface MarkdownSegment {
  type: MarkdownType;
  content: string;
}

export function parseMarkdown(content: string): MarkdownSegment[] {
  const segments: MarkdownSegment[] = [];
  const lines = content.split('\n');
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.trim().startsWith('```')) {
      let codeContent = '';
      i++;
      while (i < lines.length && !lines[i].trim().startsWith('```')) {
        if (codeContent) { codeContent += '\n' + lines[i]; }
        else { codeContent = lines[i]; }
        i++;
      }
      segments.push({ type: MarkdownType.CODE_BLOCK, content: codeContent });
      i++;
      continue;
    }
    let remaining = line;
    while (remaining.length > 0) {
      const boldMatch = remaining.match(/\*\*(.+?)\*\*/);
      const codeMatch = remaining.match(/`([^`\n]+?)`/);
      let earliest: { match: RegExpMatchArray; type: MarkdownType } | null = null;
      if (boldMatch && boldMatch.index !== undefined) {
        earliest = { match: boldMatch, type: MarkdownType.BOLD };
      }
      if (codeMatch && codeMatch.index !== undefined) {
        if (!earliest || codeMatch.index < earliest.match.index!) {
          earliest = { match: codeMatch, type: MarkdownType.CODE };
        }
      }
      if (earliest && earliest.match.index !== undefined) {
        const idx = earliest.match.index;
        if (idx > 0) {
          segments.push({ type: MarkdownType.TEXT, content: remaining.substring(0, idx) });
        }
        segments.push({ type: earliest.type, content: earliest.match[1] });
        remaining = remaining.substring(idx + earliest.match[0].length);
      } else {
        if (remaining.length > 0) {
          segments.push({ type: MarkdownType.TEXT, content: remaining });
        }
        break;
      }
    }
    i++;
  }
  return segments;
}

export function parseMessageSegments(content: string): MessageSegment[] {
  const segments: MessageSegment[] = [];
  const lines = content.split('\n');
  let currentSegment: MessageSegment | null = null;
  let inThought = false;
  let inStep = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmedLine = line.trim();

    /* 暂时屏蔽思考过程解析
    if (trimmedLine === '<思考>') {
      if (currentSegment && currentSegment.content.trim()) {
        segments.push(currentSegment);
      }
      currentSegment = { type: MessageSegmentType.THOUGHT, content: '' };
      inThought = true;
      continue;
    }

    if (trimmedLine === '</思考>') {
      if (currentSegment && inThought) {
        segments.push(currentSegment);
        currentSegment = null;
      }
      inThought = false;
      continue;
    }
    */

    if (trimmedLine.startsWith('--- Step started ---')) {
      if (currentSegment && currentSegment.content.trim()) {
        segments.push(currentSegment);
      }
      currentSegment = { type: MessageSegmentType.STEP, content: '', title: 'Step' };
      inStep = true;
      continue;
    }

    if (trimmedLine.startsWith('--- Step completed ---')) {
      if (currentSegment && inStep) {
        segments.push(currentSegment);
        currentSegment = null;
      }
      inStep = false;
      continue;
    }

    if (!currentSegment) {
      currentSegment = { type: MessageSegmentType.TEXT, content: '' };
    }

    if (currentSegment.content) {
      currentSegment.content += '\n' + line;
    } else {
      currentSegment.content = line;
    }
  }

  if (currentSegment && currentSegment.content.trim()) {
    segments.push(currentSegment);
  }

  return segments;
}
