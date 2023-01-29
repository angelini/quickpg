interface ConnectionInfo {
  user: string;
  host: string;
  port: number;
  dbname: string;
}

interface ProcessInfo {
  pid: number;
}

enum InstanceState {
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
  name: string;
  state: string;
  conn_info: ConnectionInfo;
  proc_info?: ProcessInfo;
}

interface Instance {
  name: string;
  state: InstanceState;
  connInfo: ConnectionInfo;
  procInfo?: ProcessInfo;
}

class QuickPgClient {
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
        name: instance.name,
        state: parseState(instance.state),
        connInfo: instance.conn_info,
        procInfo: instance.proc_info,
      };
    });
  }

  async status(name: string): Promise<Instance> {
    const response = await fetch(`http://${this.host}/status/${name}`, {
      method: "GET",
      headers: {
        "content-type": "application/json;charset=UTF-8",
      },
    });

    const instance = await response.json() as RawInstance;

    return {
      name: instance.name,
      state: parseState(instance.state),
      connInfo: instance.conn_info,
      procInfo: instance.proc_info,
    };
  }
}

const client = new QuickPgClient("127.0.0.1:8000");

const instances = await client.list();
console.dir(instances);

if (instances.length > 0) {
  const instance = await client.status(instances[0].name);
  console.dir(instance);
}
