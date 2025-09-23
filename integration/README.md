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
