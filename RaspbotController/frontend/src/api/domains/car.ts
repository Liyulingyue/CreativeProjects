import { apiClient } from '../client';

export type Direction = 'forward' | 'backward' | 'left' | 'right' | 'spin_left' | 'spin_right' | 'stop';

export interface MoveParams {
  direction: Direction;
  speed: number;
}

export interface MoveResponse {
  status: string;
  direction: string;
  speed: number;
}

export interface ManualParams {
  l_speed: number;
  r_speed: number;
}

export interface ManualResponse {
  status: string;
  l_speed: number;
  r_speed: number;
}

export interface ServoParams {
  id: number;
  angle: number;
}

export interface ServoResponse {
  status: string;
  id: number;
  angle: number;
}

export type BuzzerAction = 'on' | 'off' | 'beep' | 'set_freq';

export interface BuzzerParams {
  action: BuzzerAction;
  frequency?: number;
}

export interface BuzzerResponse {
  status: string;
  action: string;
  frequency?: number;
}

export const carApi = {
  move: async (params: MoveParams): Promise<MoveResponse> => {
    const response = await apiClient.post<MoveResponse>('/car/move', params);
    return response.data;
  },

  stop: async (): Promise<{ status: string }> => {
    const response = await apiClient.post<{ status: string }>('/car/stop');
    return response.data;
  },

  manual: async (params: ManualParams): Promise<ManualResponse> => {
    const response = await apiClient.post<ManualResponse>('/car/manual', params);
    return response.data;
  },

  servo: async (params: ServoParams): Promise<ServoResponse> => {
    const response = await apiClient.post<ServoResponse>('/car/servo', params);
    return response.data;
  },

  buzzer: async (params: BuzzerParams): Promise<BuzzerResponse> => {
    const response = await apiClient.post<BuzzerResponse>('/buzzer/', params);
    return response.data;
  },

  getDistance: async (): Promise<{ status: string; distance: number }> => {
    const response = await apiClient.get<{ status: string; distance: number }>('/distance/');
    return response.data;
  },

  led: async (action: string, led: number): Promise<{ status: string; action: string; led: number }> => {
    const response = await apiClient.post<{ status: string; action: string; led: number }>('/led/', {
      action,
      led,
    });
    return response.data;
  },

  followStart: async (): Promise<{ status: string }> => {
    const response = await apiClient.post<{ status: string }>('/follow/start');
    return response.data;
  },

  followStop: async (): Promise<{ status: string }> => {
    const response = await apiClient.post<{ status: string }>('/follow/stop');
    return response.data;
  },

  followStatus: async (): Promise<{ status: string; zone: string; distance: number }> => {
    const response = await apiClient.get<{ status: string; zone: string; distance: number }>('/follow/status');
    return response.data;
  },

  followConfig: async (): Promise<object> => {
    const response = await apiClient.get<object>('/follow/config');
    return response.data;
  },

  followConfigUpdate: async (cfg: object): Promise<object> => {
    const response = await apiClient.put<object>('/follow/config', cfg);
    return response.data;
  },
};
