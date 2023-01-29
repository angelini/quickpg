import { QuickPgClient } from "./client.ts";
import { setupSchema } from "./fixture.ts";

const client = new QuickPgClient("127.0.0.1:8000");

const name = await client.create();

try {
  const instance = await client.start(name);
  console.dir(instance);

  await setupSchema(instance.connInfo);
} finally {
  await client.destroy(name);
}
