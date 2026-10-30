/** Reading a drop that may contain folders.
 *
 * A browser hands a drop to the page as a flat list of files, and a
 * dropped folder arrives as one entry with no bytes in it -- so the
 * folder's shape, the part that mattered, is gone by the time the
 * page sees it. Chromium keeps the real tree behind
 * `webkitGetAsEntry()`, and this walks it.
 *
 * The result is a list of files, each with the directory it belongs
 * in relative to the folder that was dropped, so the server can put
 * it back where it came from.
 */

export interface DroppedFile {
  file: File;
  /** Directory below the drop target, e.g. `照片/2024`. Empty for a loose file. */
  relDir: string;
  /** How deep in the tree it was, for progress messages. */
  depth: number;
  /** A name of its own, so the queue can keep track of one file. */
  id: string;
}

interface Entry {
  isFile: boolean;
  isDirectory: boolean;
  name: string;
}

interface FileEntry extends Entry {
  isFile: true;
  file(cb: (f: File) => void, err: (e: unknown) => void): void;
}

interface DirEntry extends Entry {
  isDirectory: true;
  createReader(): {
    readEntries(cb: (entries: Entry[]) => void, err: (e: unknown) => void): void;
  };
}

function readFile(entry: FileEntry): Promise<File> {
  return new Promise((resolve, reject) => entry.file(resolve, reject));
}

/**
 * The entries a reader hands over, all of them.
 *
 * The reader returns at most a hundred at a time and has to be asked
 * again for the rest, so this loops until it comes back empty.
 */
function readAllChildren(entry: DirEntry): Promise<Entry[]> {
  return new Promise((resolve, reject) => {
    const reader = entry.createReader();
    const all: Entry[] = [];
    const readBatch = () => {
      reader.readEntries(entries => {
        if (!entries.length) {
          resolve(all);
          return;
        }
        all.push(...entries);
        readBatch();
      }, reject);
    };
    readBatch();
  });
}

async function walk(
  entry: Entry,
  relDir: string,
  depth: number,
  out: DroppedFile[],
): Promise<void> {
  if (entry.isFile) {
    try {
      const file = await readFile(entry as FileEntry);
      out.push({ file, relDir, depth, id: `${relDir}/${file.name}:${file.size}` });
    } catch {
      // A file the browser will not open (it was moved while we were
      // reading the tree) is skipped rather than failing the whole drop.
    }
    return;
  }
  if (entry.isDirectory) {
    const children = await readAllChildren(entry as DirEntry);
    for (const child of children) {
      await walk(child, relDir ? `${relDir}/${child.name}` : child.name, depth + 1, out);
    }
  }
}

/**
 * Everything in a drop, folders expanded.
 *
 * Falls back to the flat file list when the browser cannot give us
 * the tree, which means a dropped folder arrives as an empty file --
 * the same as it always did, but no worse.
 */
export async function collectDroppedFiles(dataTransfer: DataTransfer): Promise<DroppedFile[]> {
  const items = dataTransfer.items;
  const out: DroppedFile[] = [];

  if (items && items.length && typeof (items[0] as { webkitGetAsEntry?: unknown }).webkitGetAsEntry === 'function') {
    const entries: Entry[] = [];
    for (const item of Array.from(items)) {
      const get = (item as { webkitGetAsEntry?: () => Entry | null }).webkitGetAsEntry;
      if (!get) continue;
      const entry = get.call(item);
      if (entry) entries.push(entry);
    }
    if (entries.length) {
      for (const entry of entries) {
        // A folder dropped on its own keeps its name as the top
        // directory; loose files keep no directory at all.
        await walk(entry, entry.isDirectory ? entry.name : '', entry.isDirectory ? 1 : 0, out);
      }
      if (out.length) return out;
    }
  }

  for (const file of Array.from(dataTransfer.files ?? [])) {
    out.push({ file, relDir: '', depth: 0, id: `${file.name}:${file.size}:${file.lastModified}` });
  }
  return out;
}

/** The directory a file goes in, from a picker that chose a whole folder. */
export function relDirFromWebkitPath(webkitRelativePath: string): string {
  const parts = (webkitRelativePath || '').split('/').filter(Boolean);
  parts.pop(); // the file itself
  return parts.join('/');
}
