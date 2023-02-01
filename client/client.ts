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

const parseInstance = (raw: RawInstance): Instance => {
  return {
    id: raw.id,
    state: parseState(raw.state),
    connInfo: raw.conn_info,
    procInfo: raw.proc_info,
  };
};

export class QuickPgClient {
  constructor(readonly host: string) {}

  async list(): Promise<Instance[]> {
    const { instances } = await this.api<{ instances: RawInstance[] }>(
      "GET",
      "pg/instance",
      null,
    );

    return instances.map(parseInstance);
  }

  async create(dbname: string): Promise<Instance> {
    const instance = await this.api<RawInstance>(
      "POST",
      "pg/instance",
      JSON.stringify({ dbname }),
    );

    return parseInstance(instance);
  }

  async status(id: string): Promise<Instance> {
    const instance = await this.api<RawInstance>(
      "GET",
      `pg/instance/${id}`,
      null,
    );

    return parseInstance(instance);
  }

  async start(id: string): Promise<Instance> {
    const instance = await this.api<RawInstance>(
      "POST",
      `pg/instance/${id}/start`,
      null,
    );

    return parseInstance(instance);
  }

  async stop(id: string): Promise<void> {
    await this.api(
      "POST",
      `pg/instance/${id}/stop`,
      null,
    );
  }

  async fork(template: string): Promise<Instance> {
    const instance = await this.api<RawInstance>(
      "POST",
      `pg/instance/${template}/fork`,
      null,
    );

    return parseInstance(instance);
  }

  async destroy(id: string): Promise<void> {
    return await this.api("DELETE", `pg/instance/${id}`, null);
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

    const r = await response.json() as T;
    console.log(method, endpoint);
    console.dir(r);
    return r;
  }
}
