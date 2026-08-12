import { Fragment } from 'react';

interface BreadcrumbProps {
  path: string;
  onNavigate: (path: string) => void;
}

export function Breadcrumb({ path, onNavigate }: BreadcrumbProps) {
  if (!path) return null;

  const parts = path.split(/[/\\]/).filter(Boolean);

  return (
    <div className="flex items-center gap-1 px-4 py-2 bg-slate-50 text-xs overflow-x-auto border-b border-slate-100">
      {parts.map((part, index) => {
        const partPath = '/' + parts.slice(0, index + 1).join('/');
        const isLast = index === parts.length - 1;

        return (
          <Fragment key={partPath}>
            {index > 0 && <span className="text-slate-300 mx-1">/</span>}
            {isLast ? (
              <span className="font-semibold text-slate-700 whitespace-nowrap">{part}</span>
            ) : (
              <button
                onClick={() => onNavigate(partPath)}
                className="text-indigo-600 hover:text-indigo-700 hover:underline whitespace-nowrap"
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
