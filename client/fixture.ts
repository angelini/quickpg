import {
  Client,
  Transaction,
} from "https://deno.land/x/postgres@v0.17.0/mod.ts";

import { ConnectionInfo } from "./client.ts";

type CloseFunc = () => Promise<void>;

export const connect = async (
  connInfo: ConnectionInfo,
): Promise<[Client, CloseFunc]> => {
  const config = {
    applicationName: "quickpg-ts-client",
    database: connInfo.dbname,
    hostname: connInfo.host,
    port: connInfo.port,
    user: connInfo.user,
    password: "",
    tls: {
      enforce: false,
    },
  };

  const client = new Client(config);
  await client.connect();

  return [client, async () => {
    await client.end();
  }];
};

const execTx = async <T>(
  name: string,
  connInfo: ConnectionInfo,
  fn: (tx: Transaction) => Promise<T>,
): Promise<T> => {
  const [client, close] = await connect(connInfo);
  let tx = undefined;
  let started = false;

  try {
    tx = client.createTransaction(name);

    await tx.begin();
    started = true;

    const result = await fn(tx);

    await tx.commit();
    return result;
  } catch (err: unknown) {
    if (started && tx) {
      await tx.rollback();
    }
    throw err;
  } finally {
    close();
  }
};

export const setupSchema = async (connInfo: ConnectionInfo) => {
  await execTx("setup-schema", connInfo, async (tx) => {
    await tx.queryArray(`
      CREATE TABLE data (
        id serial PRIMARY KEY,
        value text
      )
    `);
  });
};

export const writeData = async (connInfo: ConnectionInfo) => {
  await execTx("write-data", connInfo, async (tx) => {
    for (let i = 0; i < 50; i++) {
      const value = (Math.random() + 1).toString(36).substring(16);
      await tx.queryArray(
        `
        INSERT INTO data (value) VALUES ($1)
        `,
        [value],
      );
    }
  });
};

export const countData = async (connInfo: ConnectionInfo): Promise<number> => {
  return await execTx("count-data", connInfo, async (tx) => {
    const result = await tx.queryArray(`SELECT count(*) FROM data`);
    return result.rows[0][0] as number;
  });
};
