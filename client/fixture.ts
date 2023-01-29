import { Client } from "https://deno.land/x/postgres@v0.17.0/mod.ts";

import { ConnectionInfo } from "./client.ts";

export const setupSchema = async (connInfo: ConnectionInfo) => {
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

  console.dir(config);

  const client = new Client(config);
  await client.connect();
  await client.end();
};
