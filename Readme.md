# QuickPG

A tool for quickly starting and forking Postgres databases.

Useful when building integration tests that require a real DB connection.

## Install

1. Ensure `bin/pg_ctl` is a symlink to your Postgres installation's `pg_ctl`
2. `RUST_LOG=tower_http=debug cargo run`

## Typescript Client

```typescript
const client = new QuickPgClient("127.0.0.1:8000");

const instance = await client.create("example");
// write to this instance using ${instance.connInfo}

// ensure the template instance is stopped before forking
await client.stop(instance.id);

const fork = await client.fork(instance.id);
// write to the fork using ${fork.connInfo}

await client.destroy(fork);
```

## Performance

```
Create blank instance:       ~550ms
Fork instance with few rows: ~120ms
Destroy forked instance:     ~30ms
```
