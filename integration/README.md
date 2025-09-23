# Agent + Capsule Integration

Container used for development of capsule and various agents

comes with: - codex - claude - capsule (pre-built at startup) - various capsule agents in `agents/` directory

### capsule-integration container setup

```bash
# build
docker compose up --build -d
# shell in
docker exec -it capsule-integration bash
# view dirs will show agents/ and capsule/
ls
# NOTE: agents/ and capsule/ are mounted so
# changes on your local are shared with container

# view that capsule is installed
capsule
# run capsule on an agent
capsule run claude
# in another window view logs
capsule monitor # monitor only works if capsule run something

```

### viewing logs

inside the `capsule-integration` container
`capsule` will log to `/root/.capsule/runs`.

If you `capsule run python3 agent.py` you will
see logs streaming to a directory with a datetime associated
with your run.

```
cd /root/.capsule

root@812d0ad0167f:~/.capsule/runs# ls
2025-09-23T20:42:32Z-bb9699
```

Right now there are 3 files capsule logs
`metadata.json`: tells you what command was run and when
`syscalls.jsonl`: tells you slightly rolled up traced syscalls with no filtering
`events.json`: tells you the human readable rollups displayed to the TUI
