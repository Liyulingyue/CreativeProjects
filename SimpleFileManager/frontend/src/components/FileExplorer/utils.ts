import type { FileNode } from '../../api';

export function formatSize(bytes: number): string {
  if (bytes === 0) return '';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

export function formatDate(dateStr: string): string {
  try {
    return new Date(dateStr).toLocaleDateString();
  } catch {
    return dateStr;
  }
}

export function isPreviewable(node: FileNode): boolean {
  if (node.is_dir) return false;
  return isImageFile(node) || isTextFile(node) || isDocumentFile(node) || isMediaFile(node);
}

/** Office documents the server can read. */
export function isDocumentFile(node: FileNode): boolean {
  return ['.docx', '.xlsx', '.pptx', '.pdf'].includes(node.extension.toLowerCase());
}

/** Audio and video the browser can play. */
export function isMediaFile(node: FileNode): boolean {
  return isVideoFile(node) || isAudioFile(node);
}

export function isVideoFile(node: FileNode): boolean {
  return ['.mp4', '.webm', '.ogg', '.mkv', '.mov', '.avi'].includes(node.extension.toLowerCase());
}

export function isAudioFile(node: FileNode): boolean {
  return ['.mp3', '.wav', '.flac', '.m4a', '.aac', '.ogg'].includes(node.extension.toLowerCase());
}

export function isImageFile(node: FileNode): boolean {
  const ext = node.extension.toLowerCase();
  return ['.jpg', '.jpeg', '.png', '.gif', '.webp', '.svg', '.bmp'].includes(ext);
}

export function isTextFile(node: FileNode): boolean {
  const ext = node.extension.toLowerCase();
  return ['.txt', '.md', '.json', '.yaml', '.yml', '.csv', '.xml', '.html', '.css', '.js', '.ts', '.jsx', '.tsx', '.py', '.rs', '.go', '.java', '.c', '.cpp', '.h', '.sh', '.bat', '.ps1', '.log', '.conf', '.ini', '.toml', '.env'].includes(ext);
}
