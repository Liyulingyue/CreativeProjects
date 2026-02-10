import { useCallback } from 'react';
import type { ContextMenuItem } from '../components/ui/ContextMenu';

interface UseContextMenuProps {
  currentPath: string[];
  onNavigate: (folder: string) => void;
  onMove: (oldPath: string, newPath: string, isFolder: boolean) => void;
  onDelete: (filename: string) => void;
  onDownload: (id: number, name: string) => void;
  onCreateFolder: (name: string) => void;
  onUploadClick: () => void;
  setModal: React.Dispatch<React.SetStateAction<{
    show: boolean;
    type: 'confirm' | 'prompt';
    title: string;
    message: string;
    value: string;
    onOk: (val: string) => void;
    okText?: string;
  }>>;
  onBatchDelete: (fileIds: number[]) => void;
}

export function useContextMenu({
  currentPath,
  onNavigate,
  onMove,
  onDelete,
  onDownload,
  onCreateFolder,
  onUploadClick,
  setModal,
  onBatchDelete
}: UseContextMenuProps) {
  const getContextMenuItems = useCallback((type: 'file' | 'folder' | 'background', selectedFiles: number[], data?: any): ContextMenuItem[] => {
    const items: ContextMenuItem[] = [];

    if (type === 'folder') {
      const fullPath = currentPath.length > 0 ? `${currentPath.join('/')}/${data}` : data;
      items.push(
        { label: '打开', icon: '📂', onClick: () => onNavigate(data) },
        { label: '重命名', icon: '✏️', onClick: () => {
          setModal({
            show: true,
            type: 'prompt',
            title: '重命名文件夹',
            message: '请输入新的文件夹名称:',
            value: data,
            onOk: (newName) => {
              if (newName && newName !== data) {
                const parentPath = currentPath.join('/');
                const newPath = parentPath ? `${parentPath}/${newName}` : newName;
                onMove(fullPath, newPath, true);
              }
              setModal(prev => ({ ...prev, show: false }));
            }
          });
        }},
        { label: '移动', icon: '🚚', onClick: () => {
          setModal({
            show: true,
            type: 'prompt',
            title: '移动文件夹',
            message: '请输入目标路径 (例如: documents/work):',
            value: currentPath.join('/'),
            onOk: (targetPath) => {
              if (targetPath !== undefined) {
                const newPath = targetPath ? `${targetPath}/${data}` : data;
                if (newPath !== fullPath) {
                  onMove(fullPath, newPath, true);
                }
              }
              setModal(prev => ({ ...prev, show: false }));
            }
          });
        }},
        { label: '删除', icon: '✕', danger: true, onClick: () => {
          setModal({
            show: true,
            type: 'confirm',
            title: '删除文件夹',
            message: `确定要删除文件夹 "${data}" 及其所有内容吗？`,
            value: '',
            onOk: () => {
              onDelete(fullPath);
              setModal(prev => ({ ...prev, show: false }));
            }
          });
        }}
      );
    } else if (type === 'file') {
      const file = data as { id: number; filename: string; size: number; upload_time: string; comment: string };
      const fileName = file.filename.split('/').pop() || '';
      items.push(
        { label: '下载', icon: '⬇', onClick: () => onDownload(file.id, file.filename) },
        { label: '重命名', icon: '✏️', onClick: () => {
          setModal({
            show: true,
            type: 'prompt',
            title: '重命名文件',
            message: '请输入新的文件名:',
            value: fileName,
            onOk: (newName) => {
              if (newName && newName !== fileName) {
                const prefix = currentPath.length > 0 ? `${currentPath.join('/')}/` : '';
                onMove(file.filename, prefix + newName, false);
              }
              setModal(prev => ({ ...prev, show: false }));
            }
          });
        }},
        { label: '移动', icon: '🚚', onClick: () => {
          setModal({
            show: true,
            type: 'prompt',
            title: '移动文件',
            message: '请输入目标路径 (例如: documents/backup):',
            value: currentPath.join('/'),
            onOk: (targetPath) => {
              if (targetPath !== undefined) {
                const newPath = targetPath ? `${targetPath}/${fileName}` : fileName;
                if (newPath !== file.filename) {
                  onMove(file.filename, newPath, false);
                }
              }
              setModal(prev => ({ ...prev, show: false }));
            }
          });
        }},
        { label: '删除', icon: '✕', danger: true, onClick: () => {
          setModal({
            show: true,
            type: 'confirm',
            title: '删除文件',
            message: `确定要删除文件 "${fileName}" 吗？`,
            value: '',
            onOk: () => {
              onDelete(file.filename);
              setModal(prev => ({ ...prev, show: false }));
            }
          });
        }}
      );
    } else {
      // 背景菜单
      if (selectedFiles.length > 0) {
        items.push(
          { label: `删除选中的 ${selectedFiles.length} 个文件`, icon: '🗑️', danger: true, onClick: () => {
            setModal({
              show: true,
              type: 'confirm',
              title: '批量删除文件',
              message: `确定要删除选中的 ${selectedFiles.length} 个文件吗？`,
              value: '',
              onOk: () => {
                onBatchDelete(selectedFiles);
                setModal(prev => ({ ...prev, show: false }));
              }
            });
          }}
        );
      }
      items.push(
        { label: '新建文件夹', icon: '📁', onClick: () => {
          setModal({
            show: true,
            type: 'prompt',
            title: '新建文件夹',
            message: '请输入文件夹名称:',
            value: '新建文件夹',
            onOk: (name) => {
              if (name) onCreateFolder(name);
              setModal(prev => ({ ...prev, show: false }));
            }
          });
        }},
        { label: '上传文件', icon: '📤', onClick: onUploadClick },
        { label: '刷新', icon: '🔄', onClick: () => window.location.reload() }
      );
    }

    return items;
  }, [currentPath, onNavigate, onMove, onDelete, onDownload, onCreateFolder, onUploadClick, setModal, onBatchDelete]);

  return getContextMenuItems;
}