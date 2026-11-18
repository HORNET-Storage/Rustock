# Running Rustock in Docker

The node is built once into a binary. A small runtime image runs it, with the
binary mounted from `./bin`. Servers need Docker, not Rust.

Rustock is an unaudited prototype: run it on testnet or beside rskj first.

| File | Role |
|---|---|
| `scripts/build-binary.sh` | builds `rustock-cli` and the maintenance tools for amd64 or arm64 |
| `docker/build.Dockerfile` | the build stage; exports only the binary |
| `docker/runtime.Dockerfile` | the runtime image |
| `docker-compose.yml` | runs it, configured from `.env` |
| `docker/node.toml` | node settings that are the same everywhere |

The `Dockerfile` in the repository root is a separate, self-contained image;
Compose does not use it.

## Build

```bash
scripts/build-binary.sh amd64      # or arm64
```

Writes `dist/rustock-0.1.0-<commit>-linux-<arch>` and
`dist/rustock-tools-0.1.0-<commit>-linux-<arch>.tar.gz`, each with a `.sha256`
file, and `-dirty` appended for uncommitted changes. `FEATURES=smtp` enables
peg-out alert email. `TOOLS="rewind_to block_info"` overrides the tool list.

Build each architecture on matching hardware; emulated builds are very slow
(RocksDB, LTO). Outside Docker the binary needs the glibc of Debian bookworm or
newer.

## Install and start

Copy the binary, its `.sha256` file, `docker-compose.yml`, `.env.example` and
`docker/` to the server. `uname -m` is `x86_64` for amd64, `aarch64` for arm64.

```bash
sha256sum -c rustock-0.1.0-<commit>-linux-amd64.sha256
mkdir -p bin
install -m 755 rustock-0.1.0-<commit>-linux-amd64 bin/.rustock-cli.new
mv bin/.rustock-cli.new bin/rustock-cli
cp .env.example .env               # set RUSTOCK_EXTERNAL_IP behind NAT
docker compose up -d --build
docker compose logs -f rustock
```

Without `bin/rustock-cli` Compose refuses to start. To roll back, install the
previous binary and recreate the container.

A fresh start syncs mainnet from genesis, which takes days.
`RUSTOCK_EXTRA_ARGS=--snap-sync` joins from a recent state instead.

`docker/node.toml` selects the `epoch` trie backend because the default
`single` backend aborts on an empty data directory ("This database has no trie
in it"). `epoch` keeps only recent state, so historical queries below its
retention window fail.

## Ports

| Port | Purpose | Published on | Setting |
|---|---|---|---|
| 5050/tcp | peers | all interfaces | `RUSTOCK_P2P_PORT` |
| 5051/udp | discovery | all interfaces | `RUSTOCK_DISCOVERY_PORT` |
| 4444/tcp | JSON-RPC | 127.0.0.1 | `RUSTOCK_RPC_BIND`, `RUSTOCK_RPC_PORT` |
| 9898/tcp | metrics | 127.0.0.1 | `RUSTOCK_METRICS_BIND`, `RUSTOCK_METRICS_PORT` |

- Open the peer and discovery ports in the firewall. On mainnet and testnet the
  node looks up its public IPv4 address at start (`--ip-check-url`, set through
  `RUSTOCK_EXTRA_ARGS`; `off` disables it). Behind NAT the lookup returns the
  outbound address, so set `RUSTOCK_EXTERNAL_IP` and forward both ports. With no
  address and no lookup the node advertises `127.0.0.1`.
- The node binds discovery to the peer port + 1, so `RUSTOCK_DISCOVERY_PORT`
  must equal `RUSTOCK_P2P_PORT + 1`. The container refuses to start otherwise.
- 5050 is rskj's mainnet port. For testnet use `RUSTOCK_NETWORK_ID=31`,
  `RUSTOCK_P2P_PORT=50505` and `RUSTOCK_DISCOVERY_PORT=50506`.
- Publishing on `127.0.0.1` keeps other hosts out only on Docker Engine 28.3.3
  or later.
- The node has no RPC authentication. Put an authenticating proxy in front of
  any RPC port that leaves a trusted network.

## Configuration

`.env` holds per-server settings; [`.env.example`](../.env.example) documents
each. `docker/node.toml` holds the rest; [`node.example.toml`](../node.example.toml)
documents every setting. Flags from `.env` override the file, and an unknown
key in it is an error.

| Changed | Run |
|---|---|
| `.env` | `docker compose up -d` |
| `bin/rustock-cli`, `docker/node.toml` | `docker compose up -d --force-recreate` |
| `docker/entrypoint.sh`, `docker/runtime.Dockerfile` | `docker compose up -d --build` |

Recreating the container kills a running node; stop it first (see Stop).

## Data

`RUSTOCK_DATA_DIR` holds the database, the node key and file logs.

| Value | Meaning |
|---|---|
| `rustock-data` (default) | Docker named volume |
| path starting with `/` or `./` | host directory |

A host directory must be owned by uid 10001, or the container refuses to start:

```bash
sudo install -d -o 10001 -g 10001 /srv/rustock
```

`docker compose down -v` deletes a named volume: the chain and the node key. An
imported mainnet database is about 154 GB; use local SSD.

## Logs

By default the node logs to stdout: `docker compose logs -f rustock`. Docker
rotates them at `RUSTOCK_LOG_MAX_SIZE` and `RUSTOCK_LOG_MAX_FILE`, about 500 MB
in total by default. `RUSTOCK_LOG_LEVEL` accepts per-module directives such as
`info,rustock_sync=debug`; a `RUST_LOG` variable in the container overrides it.

`RUSTOCK_LOG_TO_STDOUT=false` writes `rustock.log.<date>` into the data
directory, one file a day, never pruned. The log directory cannot be set
separately from the data directory.

## Stop

The running node ignores SIGTERM, so `docker compose stop` waits 30 seconds and
then kills it. For a clean stop, set `RUSTOCK_EXTRA_ARGS=--rpc-admin` and call
`rsk_shutdown`, which flushes the trie store and exits 0. The container then
stays stopped; `docker compose up -d` starts it again. The admin methods have
no authentication.

```bash
docker compose exec rustock bash -c '
  body="{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"rsk_shutdown\",\"params\":[]}"
  exec 3<>/dev/tcp/127.0.0.1/4444
  printf "POST / HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: ${#body}\r\nConnection: close\r\n\r\n%s" "$body" >&3
  cat <&3'
```

## Import an rskj database

Stop rskj, then import the directory that contains `blocks/` and `unitrie/`,
mounted read-only. `receipts/`, `blooms/` and `stateRoots/` are not needed. The
importer opens the datasources as RocksDB, so check that `dbKind.properties`
says `keyvalue.datasource=ROCKS_DB`.

```bash
docker compose run --rm -v /var/lib/rsk/database/mainnet:/import:ro rustock \
  --import-rskj /import --import-skip-metadata
docker compose run --rm rustock --metadata-in-memory
```

Without `--import-skip-metadata` the import rebuilds the metadata itself by
walking parent hashes, which takes hours. The import also keeps running after
a Ctrl-C: it skips to compaction instead of stopping. Detach from the terminal
rather than interrupting it, and restart from an empty data directory if it was
interrupted, because re-importing over a partial one needs room for both copies.

The import writes the state trie into the node's own database, which the
`epoch` backend refuses to start against. Set
`RUSTOCK_EXTRA_ARGS=--trie-backend single` before the next start. That backend
keeps every historical state version, so the database grows without bound.

## Maintenance tools

Offline programs from `crates/cli/examples/`, shipped in the tools tarball. The
node must be stopped: they take the database lock.

| Tool | Does |
|---|---|
| `rewind_to <dir> <block>` | resets the heads to `<block>`; the node re-syncs the tail |
| `set_exec_head <dir> <block> [--dry-run]` | moves the executed head to a canonical block |
| `reset_exec_head <dir>` | replays execution from genesis, keeping downloaded blocks |
| `block_info <dir> <height>...` | prints stored block metadata |
| `height_peek <dir> <from> [to]` | lists every block stored at a height |

```bash
mkdir -p bin/tools && tar -xzf rustock-tools-<version>-linux-amd64.tar.gz -C bin/tools
docker compose run --rm tools rewind_to /var/lib/rustock <block>
```

The tools run in the image built by `docker compose up -d --build`. Check
`set_exec_head` with `--dry-run` first. `rewind_to` needs the state at the
target block in the database.

## Monitoring

The health check opens a TCP connection to the metrics port, so it shows that
the process serves, not that it is synced. Sync progress:

```bash
curl -s localhost:9898/metrics | grep -E 'rustock_(peers|sync_height|sync_target_height) '
```

Metrics are listed in [metrics.md](metrics.md) and alerting is configured in
[alerts.md](alerts.md).
