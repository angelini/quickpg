import { QuickPgClient } from "./client.ts";
import { setupSchema } from "./fixture.ts";

const client = new QuickPgClient("127.0.0.1:8000");

const id = await client.create();

try {
  const instance = await client.start(id);
  console.dir(instance);

  await setupSchema(instance.connInfo);
} finally {
  await client.destroy(id);
}
