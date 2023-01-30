import { QuickPgClient } from "./client.ts";
import { setupSchema } from "./fixture.ts";

const client = new QuickPgClient("127.0.0.1:8000");

const instance = await client.create("example");
console.dir(instance);

try {
  await setupSchema(instance.connInfo);
} finally {
  await client.destroy(instance.id);
}
