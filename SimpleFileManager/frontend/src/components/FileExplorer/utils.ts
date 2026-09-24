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
  const ext = node.extension.toLowerCase();
  return isImageFile(node) || isTextFile(node) || ext === '.pdf';
}

export function isImageFile(node: FileNode): boolean {
  const ext = node.extension.toLowerCase();
  return ['.jpg', '.jpeg', '.png', '.gif', '.webp', '.svg', '.bmp'].includes(ext);
}

export function isTextFile(node: FileNode): boolean {
  const ext = node.extension.toLowerCase();
  return ['.txt', '.md', '.json', '.yaml', '.yml', '.csv', '.xml', '.html', '.css', '.js', '.ts', '.jsx', '.tsx', '.py', '.rs', '.go', '.java', '.c', '.cpp', '.h', '.sh', '.bat', '.ps1', '.log', '.conf', '.ini', '.toml', '.env'].includes(ext);
}
