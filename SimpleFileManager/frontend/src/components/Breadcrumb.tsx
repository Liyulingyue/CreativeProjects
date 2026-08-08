interface BreadcrumbProps {
  path: string;
  onNavigate: (path: string) => void;
}

export function Breadcrumb({ path, onNavigate }: BreadcrumbProps) {
  if (!path) return null;

  const parts = path.split(/[/\\]/).filter(Boolean);

  return (
    <div className="breadcrumb">
      {parts.map((part, index) => {
        const partPath = '/' + parts.slice(0, index + 1).join('/');
        const isLast = index === parts.length - 1;

        return (
          <span key={partPath}>
            {index > 0 && <span className="breadcrumb-separator"> / </span>}
            {isLast ? (
              <span className="breadcrumb-current">{part}</span>
            ) : (
              <span className="breadcrumb-item" onClick={() => onNavigate(partPath)}>
                {part}
              </span>
            )}
          </span>
        );
      })}
    </div>
  );
}
