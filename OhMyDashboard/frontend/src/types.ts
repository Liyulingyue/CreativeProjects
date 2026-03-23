export type SystemInfo = {
  os: string;
  os_release: string;
  cpu_percent: number;
  memory: {
    total: number;
    used: number;
    percent: number;
  };
  disk: {
    total: number;
    used: number;
    percent: number;
  };
  uptime_seconds: number;
};

export type DockerContainer = {
  id: string;
  name: string;
  status: string;
  image: string;
};

export type StartupItem = {
  name: string;
  status: string;
};

export type ProcessInfo = {
  pid: number;
  name: string;
  cpu_percent: number;
  memory_percent: number;
};
