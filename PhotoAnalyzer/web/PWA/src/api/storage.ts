import type { FileEntry, AnalysisResult } from "../types";

const DB_NAME = "photo-analyzer-db";
const DB_VERSION = 1;
const PHOTOS_STORE = "photos";
const RESULTS_STORE = "results";
const SETTINGS_STORE = "settings";

interface PhotoRecord {
  id: string;
  name: string;
  type: string;
  data: ArrayBuffer;
  thumb: string;
  addedAt: number;
}

interface ResultRecord {
  id: string;
  photoFile: string;
  result: AnalysisResult;
  analyzedAt: number;
}

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);

    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result);

    request.onupgradeneeded = (event) => {
      const db = (event.target as IDBOpenDBRequest).result;

      if (!db.objectStoreNames.contains(PHOTOS_STORE)) {
        const photosStore = db.createObjectStore(PHOTOS_STORE, { keyPath: "id" });
        photosStore.createIndex("addedAt", "addedAt", { unique: false });
      }

      if (!db.objectStoreNames.contains(RESULTS_STORE)) {
        const resultsStore = db.createObjectStore(RESULTS_STORE, { keyPath: "id" });
        resultsStore.createIndex("photoFile", "photoFile", { unique: false });
        resultsStore.createIndex("analyzedAt", "analyzedAt", { unique: false });
      }

      if (!db.objectStoreNames.contains(SETTINGS_STORE)) {
        db.createObjectStore(SETTINGS_STORE, { keyPath: "key" });
      }
    };
  });
}

export async function savePhotos(
  entries: FileEntry[],
  maxCount: number
): Promise<void> {
  const db = await openDB();

  const tx = db.transaction(PHOTOS_STORE, "readwrite");
  const store = tx.objectStore(PHOTOS_STORE);

  const allKeys = await new Promise<string[]>((resolve, reject) => {
    const request = store.getAllKeys();
    request.onsuccess = () => resolve(request.result as string[]);
    request.onerror = () => reject(request.error);
  });

  const existingIds = new Set(entries.map((e) => e.id));
  const toDelete = allKeys.filter((id) => !existingIds.has(id));

  for (const id of toDelete) {
    store.delete(id);
  }

  for (const entry of entries) {
    const buffer = await entry.file.arrayBuffer();
    const record: PhotoRecord = {
      id: entry.id,
      name: entry.file.name,
      type: entry.file.type,
      data: buffer,
      thumb: entry.thumb || "",
      addedAt: Date.now(),
    };
    store.put(record);
  }

  const countIndex = store.index("addedAt");
  const countRequest = countIndex.getAll();

  await new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });

  const allRecords: PhotoRecord[] = await new Promise((resolve, reject) => {
    const req = store.getAll();
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });

  if (allRecords.length > maxCount) {
    const sorted = allRecords.sort((a, b) => a.addedAt - b.addedAt);
    const toRemove = sorted.slice(0, sorted.length - maxCount);

    const tx2 = db.transaction(PHOTOS_STORE, "readwrite");
    const store2 = tx2.objectStore(PHOTOS_STORE);
    for (const record of toRemove) {
      store2.delete(record.id);
    }

    await new Promise<void>((resolve, reject) => {
      tx2.oncomplete = () => resolve();
      tx2.onerror = () => reject(tx2.error);
    });
  }

  db.close();
}

export async function loadPhotos(): Promise<FileEntry[]> {
  const db = await openDB();

  return new Promise((resolve, reject) => {
    const tx = db.transaction(PHOTOS_STORE, "readonly");
    const store = tx.objectStore(PHOTOS_STORE);
    const request = store.getAll();

    request.onsuccess = async () => {
      const records: PhotoRecord[] = request.result;

      const entries: FileEntry[] = [];

      for (const record of records) {
        const blob = new Blob([record.data], { type: record.type });
        const file = new File([blob], record.name, { type: record.type });
        entries.push({
          id: record.id,
          file,
          thumb: record.thumb,
        });
      }

      db.close();
      resolve(entries);
    };

    request.onerror = () => {
      db.close();
      reject(request.error);
    };
  });
}

export async function saveResults(
  results: AnalysisResult[],
  maxCount: number
): Promise<void> {
  const db = await openDB();
  const tx = db.transaction(RESULTS_STORE, "readwrite");
  const store = tx.objectStore(RESULTS_STORE);

  await new Promise<void>((resolve, reject) => {
    const req = store.clear();
    req.onsuccess = () => resolve();
    req.onerror = () => reject(req.error);
  });

  for (const result of results.slice(0, maxCount)) {
    const record: ResultRecord = {
      id: `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
      photoFile: result.file || "",
      result,
      analyzedAt: Date.now(),
    };
    store.put(record);
  }

  await new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });

  db.close();
}

export async function loadResults(): Promise<AnalysisResult[]> {
  const db = await openDB();

  return new Promise((resolve, reject) => {
    const tx = db.transaction(RESULTS_STORE, "readonly");
    const store = tx.objectStore(RESULTS_STORE);
    const request = store.getAll();

    request.onsuccess = () => {
      const records: ResultRecord[] = request.result;
      const results = records
        .sort((a, b) => b.analyzedAt - a.analyzedAt)
        .map((r) => r.result);
      db.close();
      resolve(results);
    };

    request.onerror = () => {
      db.close();
      reject(request.error);
    };
  });
}

export async function clearAllData(): Promise<void> {
  const db = await openDB();

  const stores = [PHOTOS_STORE, RESULTS_STORE, SETTINGS_STORE];

  for (const storeName of stores) {
    const tx = db.transaction(storeName, "readwrite");
    const store = tx.objectStore(storeName);
    store.clear();
    await new Promise<void>((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  }

  db.close();
}

export async function getMaxCacheCount(): Promise<number> {
  const db = await openDB();

  return new Promise((resolve, reject) => {
    const tx = db.transaction(SETTINGS_STORE, "readonly");
    const store = tx.objectStore(SETTINGS_STORE);
    const request = store.get("maxCacheCount");

    request.onsuccess = () => {
      const value = request.result;
      db.close();
      resolve(value ? value.value : 10);
    };

    request.onerror = () => {
      db.close();
      reject(request.error);
    };
  });
}

export async function setMaxCacheCount(count: number): Promise<void> {
  const db = await openDB();

  return new Promise((resolve, reject) => {
    const tx = db.transaction(SETTINGS_STORE, "readwrite");
    const store = tx.objectStore(SETTINGS_STORE);
    store.put({ key: "maxCacheCount", value: count });

    tx.oncomplete = () => {
      db.close();
      resolve();
    };

    tx.onerror = () => {
      db.close();
      reject(tx.error);
    };
  });
}
