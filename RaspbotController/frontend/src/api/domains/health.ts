import { apiClient } from '../client';

export interface HealthStatus {
  car: boolean;
  camera: boolean;
}

export const healthApi = {
  get: async (): Promise<HealthStatus> => {
    const response = await apiClient.get<HealthStatus>('/health');
    return response.data;
  },
};
