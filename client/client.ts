export interface ConnectionInfo {
  user: string;
  host: string;
  port: number;
  dbname: string;
}

export interface ProcessInfo {
  pid: number;
}

export enum InstanceState {
  Stopped,
  Running,
}

const parseState = (str: string): InstanceState => {
  switch (str) {
    case "Stopped":
      return InstanceState.Stopped;
    case "Running":
      return InstanceState.Running;
    default:
      throw new Error(`Invalid instance state: ${str}`);
  }
};

interface RawInstance {
  id: string;
  state: string;
  conn_info: ConnectionInfo;
  proc_info?: ProcessInfo;
}

export interface Instance {
  id: string;
  state: InstanceState;
  connInfo: ConnectionInfo;
  procInfo?: ProcessInfo;
}

export class QuickPgClient {
  constructor(readonly host: string) {}

  async list(): Promise<Instance[]> {
    const response = await fetch(`http://${this.host}/`, {
      method: "GET",
      headers: {
        "content-type": "application/json;charset=UTF-8",
      },
    });

    const { instances } = await response.json() as { instances: RawInstance[] };

    return instances.map((instance) => {
      return {
        id: instance.id,
        state: parseState(instance.state),
        connInfo: instance.conn_info,
        procInfo: instance.proc_info,
      };
    });
  }

  async status(id: string): Promise<Instance> {
    const response = await fetch(`http://${this.host}/status/${id}`, {
      method: "GET",
      headers: {
        "content-type": "application/json;charset=UTF-8",
      },
    });

    const instance = await response.json() as RawInstance;

    return {
      id: instance.id,
      state: parseState(instance.state),
      connInfo: instance.conn_info,
      procInfo: instance.proc_info,
    };
  }

  async create(): Promise<string> {
    const response = await fetch(`http://${this.host}/create`, {
      method: "POST",
      headers: {
        "content-type": "application/json;charset=UTF-8",
      },
    });

    const { id } = await response.json() as { id: string };
    return id;
  }

  async start(id: string): Promise<Instance> {
    const response = await fetch(`http://${this.host}/start`, {
      method: "POST",
      headers: {
        "content-type": "application/json;charset=UTF-8",
      },
      body: JSON.stringify({ id }),
    });

    const instance = await response.json() as RawInstance;

    return {
      id: instance.id,
      state: parseState(instance.state),
      connInfo: instance.conn_info,
      procInfo: instance.proc_info,
    };
  }

  async stop(id: string): Promise<void> {
    const response = await fetch(`http://${this.host}/stop`, {
      method: "POST",
      headers: {
        "content-type": "application/json;charset=UTF-8",
      },
      body: JSON.stringify({ id }),
    });

    await response.json();
  }

  async fork(template: string): Promise<Instance> {
    const response = await fetch(`http://${this.host}/fork`, {
      method: "POST",
      headers: {
        "content-type": "application/json;charset=UTF-8",
      },
      body: JSON.stringify({ id: template }),
    });

    const { id } = await response.json() as { id: string };

    return await this.status(id);
  }

  async destroy(id: string): Promise<void> {
    const response = await fetch(`http://${this.host}/destroy`, {
      method: "POST",
      headers: {
        "content-type": "application/json;charset=UTF-8",
      },
      body: JSON.stringify({ id }),
    });

    await response.json();
  }
}
