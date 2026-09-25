import type { ReactNode } from 'react';
import type { FileNode } from '../../api';

const PATHS: Record<string, ReactNode> = {
  folder: <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />,
  folderOpen: <path d="M3 7v13a1 1 0 0 0 1 1h16a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1H10L8 4H4a1 1 0 0 0-1 3z" />,
  search: <><circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" /></>,
  database: <><ellipse cx="12" cy="5" rx="9" ry="3" /><path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3" /><path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5" /></>,
  chat: <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />,
  clipboard: <><rect x="8" y="2" width="8" height="4" rx="1" /><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" /><line x1="9" y1="12" x2="15" y2="12" /><line x1="9" y1="16" x2="13" y2="16" /></>,
  newspaper: <><path d="M4 22h16a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v16a2 2 0 0 1-2 2zm0 0a2 2 0 0 1-2-2v-9c0-1.1.9-2 2-2h2" /><path d="M18 14h-8" /><path d="M15 18h-5" /><path d="M10 6h8v4h-8V6z" /></>,
  settings: <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" /></>,
  upload: <><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="17 8 12 3 7 8" /><line x1="12" y1="3" x2="12" y2="15" /></>,
  download: <><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" /></>,
  edit: <><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" /><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" /></>,
  trash: <><polyline points="3 6 5 6 21 6" /><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" /><line x1="10" y1="11" x2="10" y2="17" /><line x1="14" y1="11" x2="14" y2="17" /></>,
  move: <><polyline points="15 14 20 9 15 4" /><path d="M4 20v-7a4 4 0 0 1 4-4h12" /></>,
  refresh: <><polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" /></>,
  plus: <><line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" /></>,
  arrowLeft: <><line x1="19" y1="12" x2="5" y2="12" /><polyline points="12 19 5 12 12 5" /></>,
  chevronRight: <polyline points="9 18 15 12 9 6" />,
  chevronDown: <polyline points="6 9 12 15 18 9" />,
  x: <><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></>,
  check: <polyline points="20 6 9 17 4 12" />,
  checkCircle: <><circle cx="12" cy="12" r="10" /><polyline points="9 12 11 14 15 10" /></>,
  info: <><circle cx="12" cy="12" r="10" /><line x1="12" y1="16" x2="12" y2="12" /><line x1="12" y1="8" x2="12.01" y2="8" /></>,
  alert: <><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" /><line x1="12" y1="9" x2="12" y2="13" /><line x1="12" y1="17" x2="12.01" y2="17" /></>,
  grid: <><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /></>,
  list: <><line x1="8" y1="6" x2="21" y2="6" /><line x1="8" y1="12" x2="21" y2="12" /><line x1="8" y1="18" x2="21" y2="18" /><line x1="3" y1="6" x2="3.01" y2="6" /><line x1="3" y1="12" x2="3.01" y2="12" /><line x1="3" y1="18" x2="3.01" y2="18" /></>,
  rows: <><rect x="3" y="3" width="18" height="6" rx="1" /><rect x="3" y="15" width="18" height="6" rx="1" /></>,
  menu: <><line x1="3" y1="12" x2="21" y2="12" /><line x1="3" y1="6" x2="21" y2="6" /><line x1="3" y1="18" x2="21" y2="18" /></>,
  file: <><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><polyline points="14 2 14 8 20 8" /></>,
  fileText: <><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><polyline points="14 2 14 8 20 8" /><line x1="16" y1="13" x2="8" y2="13" /><line x1="16" y1="17" x2="8" y2="17" /><polyline points="10 9 9 9 8 9" /></>,
  image: <><rect x="3" y="3" width="18" height="18" rx="2" ry="2" /><circle cx="8.5" cy="8.5" r="1.5" /><polyline points="21 15 16 10 5 21" /></>,
  video: <><polygon points="23 7 16 12 23 17 23 7" /><rect x="1" y="5" width="15" height="14" rx="2" ry="2" /></>,
  music: <><path d="M9 18V5l12-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="16" r="3" /></>,
  code: <><polyline points="16 18 22 12 16 6" /><polyline points="8 6 2 12 8 18" /></>,
  archive: <><polyline points="21 8 21 21 3 21 3 8" /><rect x="1" y="3" width="22" height="5" /><line x1="10" y1="12" x2="14" y2="12" /></>,
  send: <><line x1="22" y1="2" x2="11" y2="13" /><polygon points="22 2 15 22 11 13 2 9 22 2" /></>,
  shield: <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />,
  camera: <><path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z" /><circle cx="12" cy="13" r="4" /></>,
  bot: <><rect x="4" y="8" width="16" height="12" rx="2" /><path d="M12 8V4" /><circle cx="12" cy="4" r="1" /><circle cx="9" cy="14" r="1" /><circle cx="15" cy="14" r="1" /></>,
  eye: <><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" /><circle cx="12" cy="12" r="3" /></>,
  copy: <><rect x="9" y="9" width="13" height="13" rx="2" ry="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></>,
  clock: <><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></>,
  hardDrive: <><line x1="22" y1="12" x2="2" y2="12" /><path d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" /><line x1="6" y1="16" x2="6.01" y2="16" /><line x1="10" y1="16" x2="10.01" y2="16" /></>,
  sparkles: <path d="M12 3l1.9 5.8a2 2 0 0 0 1.3 1.3L21 12l-5.8 1.9a2 2 0 0 0-1.3 1.3L12 21l-1.9-5.8a2 2 0 0 0-1.3-1.3L3 12l5.8-1.9a2 2 0 0 0 1.3-1.3L12 3z" />,
  inbox: <><polyline points="22 12 16 12 14 15 10 15 8 12 2 12" /><path d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" /></>,
  zap: <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />,
};

export function Icon({ name, size = 18, className }: { name: keyof typeof PATHS | string; size?: number; className?: string }) {
  const path = PATHS[name];
  if (!path) return null;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      {path}
    </svg>
  );
}

const FILE_ICON_MAP: Record<string, { icon: string; color: string }> = {
  '.jpg': { icon: 'image', color: 'text-emerald-500' },
  '.jpeg': { icon: 'image', color: 'text-emerald-500' },
  '.png': { icon: 'image', color: 'text-emerald-500' },
  '.gif': { icon: 'image', color: 'text-emerald-500' },
  '.webp': { icon: 'image', color: 'text-emerald-500' },
  '.svg': { icon: 'image', color: 'text-emerald-500' },
  '.bmp': { icon: 'image', color: 'text-emerald-500' },
  '.cr3': { icon: 'image', color: 'text-emerald-500' },
  '.raw': { icon: 'image', color: 'text-emerald-500' },
  '.mp4': { icon: 'video', color: 'text-violet-500' },
  '.avi': { icon: 'video', color: 'text-violet-500' },
  '.mkv': { icon: 'video', color: 'text-violet-500' },
  '.mov': { icon: 'video', color: 'text-violet-500' },
  '.wmv': { icon: 'video', color: 'text-violet-500' },
  '.flv': { icon: 'video', color: 'text-violet-500' },
  '.mp3': { icon: 'music', color: 'text-pink-500' },
  '.wav': { icon: 'music', color: 'text-pink-500' },
  '.ogg': { icon: 'music', color: 'text-pink-500' },
  '.flac': { icon: 'music', color: 'text-pink-500' },
  '.m4a': { icon: 'music', color: 'text-pink-500' },
  '.aac': { icon: 'music', color: 'text-pink-500' },
  '.pdf': { icon: 'fileText', color: 'text-red-500' },
  '.doc': { icon: 'fileText', color: 'text-blue-500' },
  '.docx': { icon: 'fileText', color: 'text-blue-500' },
  '.txt': { icon: 'fileText', color: 'text-slate-500' },
  '.md': { icon: 'fileText', color: 'text-slate-500' },
  '.csv': { icon: 'fileText', color: 'text-green-600' },
  '.xls': { icon: 'fileText', color: 'text-green-600' },
  '.xlsx': { icon: 'fileText', color: 'text-green-600' },
  '.ppt': { icon: 'fileText', color: 'text-orange-500' },
  '.pptx': { icon: 'fileText', color: 'text-orange-500' },
  '.zip': { icon: 'archive', color: 'text-amber-600' },
  '.tar': { icon: 'archive', color: 'text-amber-600' },
  '.gz': { icon: 'archive', color: 'text-amber-600' },
  '.7z': { icon: 'archive', color: 'text-amber-600' },
  '.rar': { icon: 'archive', color: 'text-amber-600' },
  '.json': { icon: 'code', color: 'text-slate-600' },
  '.xml': { icon: 'code', color: 'text-slate-600' },
  '.yaml': { icon: 'code', color: 'text-slate-600' },
  '.yml': { icon: 'code', color: 'text-slate-600' },
  '.html': { icon: 'code', color: 'text-slate-600' },
  '.css': { icon: 'code', color: 'text-slate-600' },
  '.js': { icon: 'code', color: 'text-slate-600' },
  '.ts': { icon: 'code', color: 'text-slate-600' },
  '.jsx': { icon: 'code', color: 'text-slate-600' },
  '.tsx': { icon: 'code', color: 'text-slate-600' },
  '.py': { icon: 'code', color: 'text-slate-600' },
  '.rs': { icon: 'code', color: 'text-slate-600' },
  '.go': { icon: 'code', color: 'text-slate-600' },
  '.java': { icon: 'code', color: 'text-slate-600' },
  '.c': { icon: 'code', color: 'text-slate-600' },
  '.cpp': { icon: 'code', color: 'text-slate-600' },
  '.h': { icon: 'code', color: 'text-slate-600' },
  '.sh': { icon: 'fileText', color: 'text-slate-600' },
  '.exe': { icon: 'hardDrive', color: 'text-slate-600' },
  '.dmg': { icon: 'hardDrive', color: 'text-slate-600' },
  '.deb': { icon: 'hardDrive', color: 'text-slate-600' },
  '.apk': { icon: 'hardDrive', color: 'text-slate-600' },
};

export function FileIcon({ node, size = 18 }: { node: FileNode; size?: number }) {
  if (node.is_dir) {
    return <Icon name="folder" size={size} className="text-amber-500" />;
  }
  const ext = node.extension.toLowerCase();
  const meta = FILE_ICON_MAP[ext] || { icon: 'file', color: 'text-slate-400' };
  return <Icon name={meta.icon} size={size} className={meta.color} />;
}
