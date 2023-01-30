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
    const { instances } = await this.api<{ instances: RawInstance[] }>(
      "GET",
      "",
      null,
    );

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
    const instance = await this.api<RawInstance>("GET", `status/${id}`, null);

    return {
      id: instance.id,
      state: parseState(instance.state),
      connInfo: instance.conn_info,
      procInfo: instance.proc_info,
    };
  }

  async create(dbname: string): Promise<Instance> {
    const instance = await this.api<RawInstance>(
      "POST",
      "create",
      JSON.stringify({ dbname }),
    );

    return {
      id: instance.id,
      state: parseState(instance.state),
      connInfo: instance.conn_info,
      procInfo: instance.proc_info,
    };
  }

  async start(id: string): Promise<Instance> {
    const instance = await this.api<RawInstance>(
      "POST",
      "start",
      JSON.stringify({ id }),
    );

    return {
      id: instance.id,
      state: parseState(instance.state),
      connInfo: instance.conn_info,
      procInfo: instance.proc_info,
    };
  }

  async stop(id: string): Promise<void> {
    await this.api<{ id: string }>(
      "POST",
      "stop",
      JSON.stringify({ id }),
    );
  }

  async fork(template: string): Promise<Instance> {
    const { id } = await this.api<{ id: string }>(
      "POST",
      "fork",
      JSON.stringify({ id: template }),
    );

    return await this.status(id);
  }

  async destroy(id: string): Promise<void> {
    return await this.api("POST", "destroy", JSON.stringify({ id }));
  }

  async api<T>(
    method: string,
    endpoint: string,
    body: string | null,
  ): Promise<T> {
    const response = await fetch(`http://${this.host}/${endpoint}`, {
      method,
      headers: {
        "content-type": "application/json;charset=UTF-8",
      },
      body,
    });

    if (!response.ok) {
      throw new Error(
        `${response.status}: ${(await response.text())}`,
      );
    }

    return response.json() as T;
  }
}
