import { Fragment } from 'react';
import { Icon } from '../ui/Icon';

interface BreadcrumbProps {
  path: string;
  onNavigate: (path: string) => void;
}

export function Breadcrumb({ path, onNavigate }: BreadcrumbProps) {
  if (!path) return null;
  const parts = path.split(/[/\\]/).filter(Boolean);

  return (
    <div className="flex items-center gap-0.5 px-4 py-2 bg-white border-b border-slate-100 text-sm overflow-x-auto flex-shrink-0">
      <Icon name="hardDrive" size={14} className="text-slate-400 mr-1 flex-shrink-0" />
      {parts.map((part, index) => {
        const partPath = '/' + parts.slice(0, index + 1).join('/');
        const isLast = index === parts.length - 1;
        return (
          <Fragment key={partPath}>
            {index > 0 && <Icon name="chevronRight" size={14} className="text-slate-300 mx-0.5 flex-shrink-0" />}
            {isLast ? (
              <span className="font-medium text-slate-700 whitespace-nowrap">{part}</span>
            ) : (
              <button
                onClick={() => onNavigate(partPath)}
                className="text-slate-500 hover:text-indigo-600 whitespace-nowrap transition-colors"
              >
                {part}
              </button>
            )}
          </Fragment>
        );
      })}
    </div>
  );
}
