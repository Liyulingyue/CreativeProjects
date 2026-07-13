import type { DirEntry } from "@/api/types";

function normalizeDirPath(path: string): string {
  const replaced = path.replace(/\//g, "\\");
  return replaced.toLowerCase();
}

function buildDuplicateMap(dirs: DirEntry[]): Map<string, DirEntry[]> {
  const grouped = new Map<string, DirEntry[]>();
  for (const dir of dirs) {
    const key = normalizeDirPath(dir.path);
    const list = grouped.get(key) ?? [];
    list.push(dir);
    grouped.set(key, list);
  }

  const duplicates = new Map<string, DirEntry[]>();
  for (const [key, list] of grouped) {
    if (list.length > 1) {
      duplicates.set(key, list);
    }
  }
  return duplicates;
}

function alertOnce(message: string, signature: string) {
  const dedupKey = `pa:dup-dirs:${signature}`;
  if (typeof window === "undefined") return;
  if (window.sessionStorage.getItem(dedupKey) === "1") return;
  window.sessionStorage.setItem(dedupKey, "1");
  window.alert(message);
}

export function reportDuplicateDirs(context: string, dirs: DirEntry[]) {
  const duplicates = buildDuplicateMap(dirs);
  if (duplicates.size === 0) return;

  const lines: string[] = [];
  for (const [, list] of duplicates) {
    lines.push(`${list[0].path} (x${list.length})`);
  }

  console.warn(`[DirGuard:${context}] 后端返回了重复目录`, {
    duplicateGroups: duplicates.size,
    duplicates: lines,
    dirs,
  });

  const signature = `${context}:${lines.join("|")}`;
  alertOnce(
    `检测到后端返回重复目录（${duplicates.size} 组）。\n${lines.slice(0, 5).join("\n")}`,
    signature
  );
}

export function appendDirUnique(
  prev: DirEntry[],
  dir: DirEntry,
  context: string
): DirEntry[] {
  const target = normalizeDirPath(dir.path);
  const exists = prev.some((d) => d.id === dir.id || normalizeDirPath(d.path) === target);
  if (!exists) return [...prev, dir];

  const samePathDifferentId = prev.some(
    (d) => d.id !== dir.id && normalizeDirPath(d.path) === target
  );
  if (samePathDifferentId) {
    reportDuplicateDirs(context, [...prev, dir]);
  }
  return prev;
}