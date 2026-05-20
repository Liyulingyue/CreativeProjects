
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
