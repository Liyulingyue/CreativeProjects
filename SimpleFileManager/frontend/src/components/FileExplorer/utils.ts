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

export function getFileIcon(node: FileNode): string {
  if (node.is_dir) return '📁';
  const ext = node.extension.toLowerCase();
  const iconMap: Record<string, string> = {
    '.jpg': '🖼️', '.jpeg': '🖼️', '.png': '🖼️', '.gif': '🖼️', '.webp': '🖼️', '.svg': '🖼️', '.bmp': '🖼️', '.cr3': '🖼️', '.raw': '🖼️',
    '.mp4': '🎬', '.avi': '🎬', '.mkv': '🎬', '.mov': '🎬', '.wmv': '🎬', '.flv': '🎬',
    '.mp3': '🎵', '.wav': '🎵', '.ogg': '🎵', '.flac': '🎵', '.m4a': '🎵', '.aac': '🎵',
    '.pdf': '📕', '.doc': '📝', '.docx': '📝',
    '.xls': '📊', '.xlsx': '📊',
    '.ppt': '📊', '.pptx': '📊',
    '.zip': '📦', '.tar': '📦', '.gz': '📦', '.7z': '📦', '.rar': '📦',
    '.txt': '📃', '.md': '📃', '.csv': '📃',
    '.json': '📋', '.xml': '📋', '.yaml': '📋', '.yml': '📋',
    '.html': '🌐', '.css': '🎨', '.js': '💻', '.ts': '💻', '.jsx': '💻', '.tsx': '💻',
    '.py': '🐍', '.rs': '🦀', '.go': '🐹', '.java': '☕', '.c': '🔧', '.cpp': '🔧', '.h': '🔧',
    '.sh': '⚙️', '.bat': '⚙️', '.ps1': '⚙️',
    '.exe': '⚙️', '.dmg': '⚙️', '.deb': '⚙️', '.apk': '⚙️',
  };
  return iconMap[ext] || '📄';
}

export function isPreviewable(node: FileNode): boolean {
  if (node.is_dir) return false;
  const ext = node.extension.toLowerCase();
  const previewable = [
    '.jpg', '.jpeg', '.png', '.gif', '.webp', '.svg', '.bmp',
    '.txt', '.md', '.json', '.yaml', '.yml', '.csv', '.xml', '.html', '.css', '.js', '.ts', '.py', '.rs', '.go', '.java', '.c', '.cpp', '.h', '.sh',
    '.pdf',
  ];
  return previewable.includes(ext);
}

export function isImageFile(node: FileNode): boolean {
  const ext = node.extension.toLowerCase();
  return ['.jpg', '.jpeg', '.png', '.gif', '.webp', '.svg', '.bmp'].includes(ext);
}

export function isTextFile(node: FileNode): boolean {
  const ext = node.extension.toLowerCase();
  return ['.txt', '.md', '.json', '.yaml', '.yml', '.csv', '.xml', '.html', '.css', '.js', '.ts', '.jsx', '.tsx', '.py', '.rs', '.go', '.java', '.c', '.cpp', '.h', '.sh', '.bat', '.ps1', '.log', '.conf', '.ini', '.toml', '.env'].includes(ext);
}
