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
  vendor_preset?: string;
  description?: string;
  active_state?: string;
  sub_state?: string;
  load_state?: string;
  main_pid?: string;
  memory_current?: string;
  cpu?: string;
};

export type ProcessInfo = {
  pid: number;
  name: string;
  username: string;
  cpu_percent: number;
  memory_percent: number;
  status: string;
  create_time?: string;
  num_threads?: number;
};

export type NetworkInterface = {
  address: string[];
  mac: string[];
  mtu: number;
  is_up: boolean;
  bytes_sent: number;
  bytes_recv: number;
  packets_sent: number;
  packets_recv: number;
  errin: number;
  errout: number;
};

export type NetworkInfo = {
  total: {
    bytes_sent: number;
    bytes_recv: number;
    packets_sent: number;
    packets_recv: number;
    errin: number;
    errout: number;
    dropin: number;
    dropout: number;
  };
  interfaces: Record<string, NetworkInterface>;
  connections: {
    total: number;
    tcp: number;
    udp: number;
  };
  listening_ports: ListeningPort[];
  active_connections: ActiveConnection[];
};

export type ListeningPort = {
  port: number;
  protocol: string;
  pid: number | null;
  process: string;
  address: string;
};

export type ActiveConnection = {
  protocol: string;
  laddr: string;
  raddr: string;
  status: string;
  pid: number | null;
};
