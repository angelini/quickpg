import { QuickPgClient } from "./client.ts";
import { setupSchema, writeData } from "./fixture.ts";

const _sleep = (ms: number) => {
  return new Promise((resolve) => setTimeout(resolve, ms));
};

const perfMark = (name: string) => {
  console.log(`> ${name}`);
  performance.mark(name);
};

const perfStart = (name: string) => {
  const full = `start-${name}`;
  perfMark(full);
};

const perfStop = (name: string) => {
  const full = `stop-${name}`;
  perfMark(full);
  performance.measure(name, `start-${name}`, `stop-${name}`);
};

const formatEntries = (entries: PerformanceEntryList): string => {
  if (!entries) {
    return `duration_ms: 0`;
  }

  let count = 0;
  let sum = 0;
  let max = 0;

  for (const entry of entries) {
    count += 1;
    sum += entry.duration;
    max = Math.max(max, entry.duration);
  }

  const avg = Math.round(sum / count * 100) / 100;

  return `duration_ms: ${avg}ms avg, ${Math.round(100 * max) / 100}ms max`;
};

const client = new QuickPgClient("127.0.0.1:8000");

perfStart("init");
const instance = await client.create("example");
perfStop("init");

try {
  await setupSchema(instance.connInfo);
  await writeData(instance.connInfo);

  perfStart("core-stop");
  await client.stop(instance.id);
  perfStop("core-stop");

  for (let i = 0; i < 10; i++) {
    perfStart("fork");
    const forkedInstance = await client.fork(instance.id);
    perfStop("fork");
    await writeData(forkedInstance.connInfo);
    perfStart("destroy-fork");
    await client.destroy(forkedInstance.id);
    perfStop("destroy-fork");
  }
} finally {
  await client.destroy(instance.id);
}

console.log("\n----------\n");

console.log("Init:", formatEntries(performance.getEntriesByName("init")));
console.log(
  "Core Stop:",
  formatEntries(performance.getEntriesByName("core-stop")),
);
console.log("Fork:", formatEntries(performance.getEntriesByName("fork")));
console.log(
  "Destroy Fork:",
  formatEntries(performance.getEntriesByName("destroy-fork")),
);
