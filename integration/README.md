# Agent + Capsule Integration

Container used for development of capsule and various agents with database integration.

Comes with: - codex - claude - capsule (pre-built at startup) - various capsule agents in `agents/` directory - PostgreSQL database for data persistence

## Quick Start

### capsule-integration container setup

```bash
# build and start cluster (capsule + database)
docker compose up --build -d

# shell into capsule container
docker exec -it capsule-integration bash

# view dirs will show agents/ and capsule/
ls
# NOTE: agents/ and capsule/ are mounted so
# changes on your local are shared with container

# view that capsule is installed
capsule --help

# run capsule on an agent
capsule run claude

# in another window view live monitoring
capsule monitor # monitor only works if capsule run something

# transfer run data to database
capsule transfer
```

## Database Integration

### Architecture

The system uses PostgreSQL (via Supabase) to store and analyze capsule run data:

- **`runs`** - Core run metadata (session ID, command, timestamps, agent type)
- **`syscall_events`** - Individual syscall records with process context
- **`actions`** - Aggregated high-level actions derived from syscalls

### Database Connection

- **Host**: `supabase-db` (within Docker network)
- **Database**: `postgres`
- **Username**: `postgres`
- **Password**: `postgres`
- **External Port**: `localhost:54322` (from host machine)

### Database Dashboard Access

Several ways to view and interact with the database:

#### 1. Command Line (Quick Access)
```bash
# From inside supabase-db container
docker exec -it supabase-db psql -U postgres -d postgres

# From host machine
psql -h localhost -p 54322 -U postgres -d postgres
```

#### 2. GUI Database Tools (Recommended)
Connect to `localhost:54322` with these tools:
- **pgAdmin** - Full-featured PostgreSQL admin tool
- **DBeaver** - Universal database tool (free)
- **TablePlus** - Modern database client (Mac/Windows)
- **DataGrip** - JetBrains database IDE

#### 3. Web-based Adminer (Lightweight)
```bash
# Run Adminer container connected to database
docker run --rm --link supabase-db:db -p 8080:8080 adminer

# Visit http://localhost:8080
# System: PostgreSQL
# Server: db
# Username: postgres
# Password: postgres
# Database: postgres
```

#### 4. Connection Settings for GUI Tools
```
Host: localhost
Port: 54322
Database: postgres
Username: postgres
Password: postgres
SSL Mode: disable (for local development)
```

### Capsule Transfer Feature

Transfer local runs to the database:

```bash
# Preview what would be transferred
capsule transfer --dry-run

# Transfer all new runs
capsule transfer

# Transfer specific run
capsule transfer 2025-09-25T02:14:58Z-c12312
```

### Duplicate Prevention Mechanism

The system prevents duplicate transfers through a **dual-layer approach**:

1. **Database-Level**: Uses session ID as primary key with `INSERT...ON CONFLICT (id) DO UPDATE`
2. **Local State**: Maintains `~/.capsule/transfer_state.json` tracking successfully transferred runs

Before each transfer:
- Checks local state file for already-transferred runs
- Queries database with `SELECT EXISTS(SELECT 1 FROM runs WHERE id = $1)`
- Skips runs that exist in either location

This ensures no data duplication while being resilient to state file corruption.

### Example Queries

#### Get Most Recent Session
```sql
-- Connect to database
psql -h supabase-db -U postgres -d postgres

-- Query the most recent capsule session
SELECT
    id,
    command_line,
    agent_type,
    start_time,
    total_syscalls
FROM runs
ORDER BY start_time DESC
LIMIT 1;
```

#### Analyze Agent Usage
```sql
-- Analyze Claude usage patterns
SELECT
    DATE(start_time) as date,
    COUNT(*) as runs,
    AVG(total_syscalls) as avg_syscalls
FROM runs
WHERE agent_type = 'claude'
GROUP BY DATE(start_time)
ORDER BY date DESC;
```

#### Get Syscalls from Latest Run
```sql
-- Get syscall events from most recent run
SELECT
    timestamp_us,
    pid,
    syscall,
    raw_line
FROM syscall_events
WHERE run_id = (SELECT id FROM runs ORDER BY start_time DESC LIMIT 1)
ORDER BY timestamp_us
LIMIT 10;
```

### Updating Database Schema

To modify the database schema:

1. **Update schema files**: Edit `supabase/schema.sql`
2. **Apply changes**:
   ```sql
   psql -h supabase-db -U postgres -d postgres
   ALTER TABLE runs ADD COLUMN new_field TEXT;
   ```
3. **Update transfer code**: Modify Rust code to handle new fields

## Log Storage

### Local Storage (Before Transfer)

Inside the `capsule-integration` container, `capsule` logs to `/root/.capsule/runs/`.

If you `capsule run python3 agent.py`, you'll see logs in a directory with datetime:

```bash
cd /root/.capsule/runs
ls
# 2025-09-25T02:14:58Z-c12312

cd 2025-09-25T02:14:58Z-c12312
ls
# metadata.json    syscalls.jsonl    events.jsonl
```

**File Contents:**
- `metadata.json`: Command run and timing information
- `syscalls.jsonl`: Raw traced syscalls with minimal filtering
- `events.jsonl`: Human-readable rollups displayed in TUI

### Database Storage (After Transfer)

After running `capsule transfer`, this data becomes queryable in PostgreSQL:
- **Run metadata** → `runs` table
- **Syscall events** → `syscall_events` table
- **Aggregated actions** → `actions` table

This enables powerful analytics, historical analysis, and AI-powered querying of capsule execution patterns.

## Development Workflow

1. **Start cluster**: `docker compose up --build -d`
2. **Enter container**: `docker exec -it capsule-integration bash`
3. **Test capsule**: `capsule run echo "test"`
4. **View local logs**: `ls /root/.capsule/runs/`
5. **Transfer to DB**: `capsule transfer`
6. **Query data**: Use psql to analyze results
7. **Iterate**: Make changes and rebuild with `cargo install --path cli --force`