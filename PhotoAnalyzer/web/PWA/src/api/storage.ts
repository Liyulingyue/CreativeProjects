import type { AnalysisResult } from "./photoAnalyzer";

const DB_NAME = "photo-analyzer-db";
const DB_VERSION = 2;
const RECORDS_STORE = "records";
const SETTINGS_STORE = "settings";

export interface RecordEntry {
  id: string;
  fileName: string;
  fileType: string;
  data: ArrayBuffer;
  thumb: string;
  addedAt: number;
  result: AnalysisResult | null;
  analyzedAt: number | null;
  failedAt: number | null;
}

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);

    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result);

    request.onupgradeneeded = (event) => {
      const db = (event.target as IDBOpenDBRequest).result;

      if (!db.objectStoreNames.contains(RECORDS_STORE)) {
        const store = db.createObjectStore(RECORDS_STORE, { keyPath: "id" });
        store.createIndex("addedAt", "addedAt", { unique: false });
        store.createIndex("analyzedAt", "analyzedAt", { unique: false });
      }

      if (!db.objectStoreNames.contains(SETTINGS_STORE)) {
        db.createObjectStore(SETTINGS_STORE, { keyPath: "key" });
      }

      if (event.oldVersion < 2) {
        const stores = Array.from(db.objectStoreNames);
        if (stores.includes("photos")) db.deleteObjectStore("photos");
        if (stores.includes("results")) db.deleteObjectStore("results");
      }
    };
  });
}

export async function saveRecord(
  record: Omit<RecordEntry, "addedAt" | "analyzedAt" | "result"> & {
    result?: AnalysisResult | null;
  }
): Promise<void> {
  const db = await openDB();
  const tx = db.transaction(RECORDS_STORE, "readwrite");
  const store = tx.objectStore(RECORDS_STORE);

  const existing = await new Promise<RecordEntry | null>((resolve) => {
    const req = store.get(record.id);
    req.onsuccess = () => resolve(req.result || null);
    req.onerror = () => resolve(null);
  });

  const finalRecord: RecordEntry = {
    id: record.id,
    fileName: record.fileName,
    fileType: record.fileType,
    data: record.data,
    thumb: record.thumb,
    addedAt: existing?.addedAt || Date.now(),
    result: record.result !== undefined ? record.result : existing?.result || null,
    analyzedAt:
      record.result !== undefined
        ? record.result?.success
          ? Date.now()
          : null
        : existing?.analyzedAt || null,
    failedAt:
      record.result !== undefined
        ? record.result?.success
          ? null
          : Date.now()
        : existing?.failedAt || null,
  };

  store.put(finalRecord);

  await new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });

  db.close();
}

export async function saveRecords(
  records: Array<{
    id: string;
    fileName: string;
    fileType: string;
    data: ArrayBuffer;
    thumb: string;
    result?: AnalysisResult | null;
  }>,
  maxCount: number
): Promise<void> {
  const db = await openDB();

  for (const record of records) {
    const tx = db.transaction(RECORDS_STORE, "readwrite");
    const store = tx.objectStore(RECORDS_STORE);

    const existing = await new Promise<RecordEntry | null>((resolve) => {
      const req = store.get(record.id);
      req.onsuccess = () => resolve(req.result || null);
      req.onerror = () => resolve(null);
    });

    const finalRecord: RecordEntry = {
      id: record.id,
      fileName: record.fileName,
      fileType: record.fileType,
      data: record.data,
      thumb: record.thumb,
      addedAt: existing?.addedAt || Date.now(),
      result: record.result !== undefined ? record.result : existing?.result || null,
      analyzedAt:
        record.result !== undefined
          ? record.result
            ? Date.now()
            : null
          : existing?.analyzedAt || null,
      failedAt:
        record.result !== undefined
          ? record.result
            ? null
            : Date.now()
          : existing?.failedAt || null,
    };

    store.put(finalRecord);

    await new Promise<void>((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  }

  await trimRecords(db, maxCount);

  db.close();
}

async function trimRecords(db: IDBDatabase, maxCount: number): Promise<void> {
  const tx = db.transaction(RECORDS_STORE, "readonly");
  const store = tx.objectStore(RECORDS_STORE);

  const allRecords: RecordEntry[] = await new Promise((resolve, reject) => {
    const req = store.getAll();
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });

  const analyzed = allRecords.filter((r) => r.analyzedAt);

  if (analyzed.length > maxCount) {
    const sorted = analyzed.sort((a, b) => (a.analyzedAt || 0) - (b.analyzedAt || 0));
    const toRemove = sorted.slice(0, analyzed.length - maxCount);

    const tx2 = db.transaction(RECORDS_STORE, "readwrite");
    const store2 = tx2.objectStore(RECORDS_STORE);
    for (const record of toRemove) {
      store2.delete(record.id);
    }

    await new Promise<void>((resolve, reject) => {
      tx2.oncomplete = () => resolve();
      tx2.onerror = () => reject(tx2.error);
    });
  }
}

export async function loadRecords(maxCount: number = 100): Promise<RecordEntry[]> {
  const db = await openDB();

  return new Promise((resolve, reject) => {
    const tx = db.transaction(RECORDS_STORE, "readonly");
    const store = tx.objectStore(RECORDS_STORE);
    const request = store.getAll();

    request.onsuccess = () => {
      const records: RecordEntry[] = request.result;
      const sorted = records
        .sort((a, b) => {
          if (a.analyzedAt && b.analyzedAt) return b.analyzedAt - a.analyzedAt;
          if (a.analyzedAt) return -1;
          if (b.analyzedAt) return 1;
          return b.addedAt - a.addedAt;
        })
        .slice(0, maxCount);
      db.close();
      resolve(sorted);
    };

    request.onerror = () => {
      db.close();
      reject(request.error);
    };
  });
}

export async function deleteRecord(id: string): Promise<void> {
  const db = await openDB();
  const tx = db.transaction(RECORDS_STORE, "readwrite");
  const store = tx.objectStore(RECORDS_STORE);
  store.delete(id);

  await new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });

  db.close();
}

export async function updateRecordResult(
  id: string,
  result: AnalysisResult
): Promise<void> {
  const db = await openDB();
  const tx = db.transaction(RECORDS_STORE, "readwrite");
  const store = tx.objectStore(RECORDS_STORE);

  const existing = await new Promise<RecordEntry | null>((resolve) => {
    const req = store.get(id);
    req.onsuccess = () => resolve(req.result || null);
    req.onerror = () => resolve(null);
  });

  if (existing) {
    existing.result = result;
    existing.analyzedAt = Date.now();
    store.put(existing);
  }

  await new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });

  db.close();
}

export async function clearAllRecords(): Promise<void> {
  const db = await openDB();
  const tx = db.transaction(RECORDS_STORE, "readwrite");
  const store = tx.objectStore(RECORDS_STORE);
  store.clear();

  await new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });

  db.close();
}

export async function clearAnalyzedRecords(): Promise<void> {
  const db = await openDB();
  const tx = db.transaction(RECORDS_STORE, "readwrite");
  const store = tx.objectStore(RECORDS_STORE);

  const allRecords: RecordEntry[] = await new Promise((resolve, reject) => {
    const req = store.getAll();
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });

  for (const record of allRecords) {
    if (record.analyzedAt) {
      store.delete(record.id);
    }
  }

  await new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });

  db.close();
}

export async function clearAllData(): Promise<void> {
  const db = await openDB();
  const stores = [RECORDS_STORE, SETTINGS_STORE];
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