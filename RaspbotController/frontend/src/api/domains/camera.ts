export const API_BASE = '/api/v1';

export const cameraApi = {
  getSnapshotUrl: (quality: number = 80) => `${API_BASE}/camera/snapshot?q=${quality}`,
  getStreamUrl: () => `${API_BASE}/camera/stream`,
};
