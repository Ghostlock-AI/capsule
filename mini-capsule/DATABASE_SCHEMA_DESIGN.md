# Database Schema Design for Syscall Forensics

## Overview
Simple schema for storing traced sessions and syscalls in Supabase/PostgreSQL for forensic analysis.

---

## Schema Design

### Table: `sessions`
Stores metadata about each trace session.

```sql
CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ NOT NULL,
    end_timestamp TIMESTAMPTZ,
    os TEXT NOT NULL,
    chipset TEXT NOT NULL,
    working_dir TEXT NOT NULL,
    program TEXT NOT NULL,
    args TEXT NOT NULL,

    -- Storage paths in Supabase Storage
    raw_trace_path TEXT,          -- e.g., "sessions/{session_id}/raw_trace.txt"
    failed_parse_path TEXT,       -- e.g., "sessions/{session_id}/failed_parse_raw.txt"

    -- Optional metadata
    total_syscalls INTEGER DEFAULT 0,
    parsed_syscalls INTEGER DEFAULT 0,
    failed_parses INTEGER DEFAULT 0,

    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Index for time-based queries
CREATE INDEX idx_sessions_timestamp ON sessions(timestamp DESC);
CREATE INDEX idx_sessions_program ON sessions(program);
```

### Table: `syscalls`
Stores individual parsed syscalls.

```sql
CREATE TABLE syscalls (
    id BIGSERIAL PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    timestamp TEXT NOT NULL,  -- Store as-is from trace (HH:MM:SS.microseconds)
    pid INTEGER,
    syscall_number INTEGER NOT NULL,
    syscall_name TEXT NOT NULL,
    raw_args JSONB NOT NULL,  -- Store as JSON array for flexibility
    raw_return TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('Process', 'File', 'Network', 'Unknown')),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX idx_syscalls_session_id ON syscalls(session_id);
CREATE INDEX idx_syscalls_category ON syscalls(category);
CREATE INDEX idx_syscalls_name ON syscalls(syscall_name);
CREATE INDEX idx_syscalls_session_category ON syscalls(session_id, category);

-- GIN index for JSON args (enables searching within args)
CREATE INDEX idx_syscalls_raw_args ON syscalls USING GIN (raw_args);
```

---

## Supabase Storage Setup

### Storage Bucket Configuration

```sql
-- Create storage bucket for trace files
INSERT INTO storage.buckets (id, name, public)
VALUES ('trace-files', 'trace-files', false);

-- Set up RLS policies (adjust based on your auth needs)
CREATE POLICY "Authenticated users can upload trace files"
ON storage.objects FOR INSERT
TO authenticated
WITH CHECK (bucket_id = 'trace-files');

CREATE POLICY "Authenticated users can read trace files"
ON storage.objects FOR SELECT
TO authenticated
USING (bucket_id = 'trace-files');
```

### Storage Structure

```
trace-files/
├── {session_id}/
│   ├── raw_trace.txt
│   └── failed_parse_raw.txt
```

### Uploading Files

**Python Example:**
```python
from supabase import create_client

supabase = create_client(SUPABASE_URL, SUPABASE_KEY)
session_id = "f3827d56-926c-4471-ab99-41abcf1a3953"

# Upload raw trace
with open("raw_trace.txt", "rb") as f:
    supabase.storage.from_("trace-files").upload(
        f"{session_id}/raw_trace.txt",
        f,
        file_options={"content-type": "text/plain"}
    )

# Upload failed parses
with open("failed_parse_raw.txt", "rb") as f:
    supabase.storage.from_("trace-files").upload(
        f"{session_id}/failed_parse_raw.txt",
        f,
        file_options={"content-type": "text/plain"}
    )

# Update session record with paths
supabase.table("sessions").update({
    "raw_trace_path": f"{session_id}/raw_trace.txt",
    "failed_parse_path": f"{session_id}/failed_parse_raw.txt"
}).eq("id", session_id).execute()
```

**Rust Example (for mini-capsule):**
```rust
// In Cargo.toml, add:
// supabase-storage = "0.1"
// reqwest = { version = "0.11", features = ["json", "multipart"] }

use reqwest::multipart;

async fn upload_trace_files(
    session_id: &str,
    raw_trace_path: &Path,
    failed_parse_path: &Path,
) -> Result<()> {
    let client = reqwest::Client::new();
    let storage_url = format!("{}/storage/v1/object/trace-files", SUPABASE_URL);

    // Upload raw_trace.txt
    let raw_trace = tokio::fs::read(raw_trace_path).await?;
    let form = multipart::Form::new()
        .text("path", format!("{}/raw_trace.txt", session_id))
        .part("file", multipart::Part::bytes(raw_trace)
            .file_name("raw_trace.txt")
            .mime_str("text/plain")?);

    client.post(&storage_url)
        .header("Authorization", format!("Bearer {}", SUPABASE_KEY))
        .multipart(form)
        .send()
        .await?;

    // Upload failed_parse_raw.txt
    let failed_parse = tokio::fs::read(failed_parse_path).await?;
    let form = multipart::Form::new()
        .text("path", format!("{}/failed_parse_raw.txt", session_id))
        .part("file", multipart::Part::bytes(failed_parse)
            .file_name("failed_parse_raw.txt")
            .mime_str("text/plain")?);

    client.post(&storage_url)
        .header("Authorization", format!("Bearer {}", SUPABASE_KEY))
        .multipart(form)
        .send()
        .await?;

    Ok(())
}
```

### Retrieving Files

**Download URL:**
```python
# Get signed URL (valid for 1 hour)
url = supabase.storage.from_("trace-files").create_signed_url(
    f"{session_id}/raw_trace.txt",
    expires_in=3600
)

# Download file
response = requests.get(url['signedURL'])
content = response.text
```

**Direct Download:**
```python
# Download file bytes
file_bytes = supabase.storage.from_("trace-files").download(
    f"{session_id}/raw_trace.txt"
)
```

---

## Example Data

### Session Record
```json
{
  "id": "f3827d56-926c-4471-ab99-41abcf1a3953",
  "timestamp": "2025-10-02T21:00:22.333470Z",
  "end_timestamp": "2025-10-02T21:00:22.591068Z",
  "os": "Debian GNU/Linux 11",
  "chipset": "aarch64",
  "working_dir": "/working/mini-capsule",
  "program": "python3",
  "args": "scripts/process_syscalls.py",
  "raw_trace_path": "f3827d56-926c-4471-ab99-41abcf1a3953/raw_trace.txt",
  "failed_parse_path": "f3827d56-926c-4471-ab99-41abcf1a3953/failed_parse_raw.txt",
  "total_syscalls": 467,
  "parsed_syscalls": 432,
  "failed_parses": 35
}
```

### Syscall Record
```json
{
  "id": 1,
  "session_id": "f3827d56-926c-4471-ab99-41abcf1a3953",
  "timestamp": "22:35:26.012973",
  "pid": null,
  "syscall_number": 198,
  "syscall_name": "socket",
  "raw_args": ["AF_INET", "SOCK_STREAM|SOCK_CLOEXEC", "IPPROTO_IP"],
  "raw_return": "3<TCP:[18898905]>",
  "category": "Network"
}
```

---

## Forensic Queries

### 1. "What external sources did the agent network with in the last session?"

```sql
-- Get the most recent session
WITH latest_session AS (
    SELECT id
    FROM sessions
    ORDER BY timestamp DESC
    LIMIT 1
)
-- Find all connect/sendto/recvfrom syscalls with addresses
SELECT
    s.timestamp,
    s.syscall_name,
    s.raw_args,
    s.raw_return
FROM syscalls s
JOIN latest_session ls ON s.session_id = ls.id
WHERE s.category = 'Network'
  AND s.syscall_name IN ('connect', 'sendto', 'recvfrom', 'bind')
  AND s.raw_args::text LIKE '%inet_addr%'
ORDER BY s.timestamp;
```

**Expected Output:**
```
timestamp         | syscall_name | raw_args                                      | raw_return
------------------|--------------|-----------------------------------------------|------------------
22:41:39.690808   | connect      | ["4<TCP:[..]>", "{sa_family=AF_INET, ...}"]  | 0
22:41:39.695585   | sendto       | ["4<TCP:[..]>", "Hello from client!", ...]   | 18
```

### 2. "What files were read from outside the project directory?"

```sql
-- Get the most recent session
WITH latest_session AS (
    SELECT id, working_dir
    FROM sessions
    ORDER BY timestamp DESC
    LIMIT 1
)
-- Find openat/read syscalls with paths outside working_dir
SELECT
    s.timestamp,
    s.syscall_name,
    s.raw_args->>1 AS filepath,  -- Second arg is usually the path
    s.raw_return
FROM syscalls s
JOIN latest_session ls ON s.session_id = ls.id
WHERE s.category = 'File'
  AND s.syscall_name IN ('openat', 'read', 'readlinkat')
  AND s.raw_args->>1 NOT LIKE ls.working_dir || '%'
  AND s.raw_args->>1 LIKE '"%/%"'  -- Contains a path
ORDER BY s.timestamp;
```

### 3. "Did the agent write any new files outside the project directory?"

```sql
WITH latest_session AS (
    SELECT id, working_dir
    FROM sessions
    ORDER BY timestamp DESC
    LIMIT 1
),
write_syscalls AS (
    SELECT
        s.timestamp,
        s.syscall_name,
        s.raw_args->>1 AS filepath,
        s.raw_return
    FROM syscalls s
    JOIN latest_session ls ON s.session_id = ls.id
    WHERE s.category = 'File'
      AND s.syscall_name IN ('openat', 'write')
      -- Check for O_CREAT flag (indicates file creation)
      AND (s.raw_args::text LIKE '%O_CREAT%' OR s.syscall_name = 'write')
      -- Outside project directory
      AND s.raw_args->>1 NOT LIKE ls.working_dir || '%'
      AND s.raw_args->>1 LIKE '"%/%"'
)
SELECT DISTINCT filepath, MIN(timestamp) as first_write
FROM write_syscalls
GROUP BY filepath
ORDER BY first_write;
```

### 4. "Show all syscalls for a specific session"

```sql
SELECT
    s.timestamp,
    s.syscall_name,
    s.category,
    s.raw_args,
    s.raw_return
FROM syscalls s
WHERE s.session_id = 'f3827d56-926c-4471-ab99-41abcf1a3953'
ORDER BY s.id;
```

### 5. "Find all failed syscalls (errors)"

```sql
WITH latest_session AS (
    SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1
)
SELECT
    s.timestamp,
    s.syscall_name,
    s.category,
    s.raw_args,
    s.raw_return
FROM syscalls s
JOIN latest_session ls ON s.session_id = ls.id
WHERE s.raw_return LIKE '%-1%'  -- Error return codes
ORDER BY s.timestamp;
```

### 6. "Get session with raw trace files"

```sql
-- Get session info including file URLs
SELECT
    s.id,
    s.timestamp,
    s.program,
    s.total_syscalls,
    s.parsed_syscalls,
    s.failed_parses,
    s.raw_trace_path,
    s.failed_parse_path,
    -- Calculate parse success rate
    ROUND(100.0 * s.parsed_syscalls / NULLIF(s.total_syscalls, 0), 2) as parse_success_rate
FROM sessions s
ORDER BY s.timestamp DESC
LIMIT 1;
```

Then retrieve files from storage:
```python
session = query_result[0]
raw_trace_url = supabase.storage.from_("trace-files").create_signed_url(
    session['raw_trace_path'],
    expires_in=3600
)['signedURL']
```

### 7. "Get full session context for forensic analysis"

```sql
-- Complete session information
WITH latest_session AS (
    SELECT * FROM sessions ORDER BY timestamp DESC LIMIT 1
)
SELECT
    ls.*,
    COUNT(sc.id) as syscall_count,
    COUNT(sc.id) FILTER (WHERE sc.category = 'Network') as network_count,
    COUNT(sc.id) FILTER (WHERE sc.category = 'File') as file_count,
    COUNT(sc.id) FILTER (WHERE sc.category = 'Process') as process_count
FROM latest_session ls
LEFT JOIN syscalls sc ON sc.session_id = ls.id
GROUP BY ls.id, ls.timestamp, ls.end_timestamp, ls.os, ls.chipset,
         ls.working_dir, ls.program, ls.args, ls.raw_trace_path,
         ls.failed_parse_path, ls.total_syscalls, ls.parsed_syscalls,
         ls.failed_parses, ls.created_at;
```

---

## Natural Language to SQL Options

### 1. **Supabase + Claude/GPT (Recommended - Simplest)**

**How it works:**
- Give Claude/GPT the schema + natural language question
- LLM generates SQL query
- Execute query via Supabase client
- Return results in natural language

**Pros:**
- No additional infrastructure needed
- Works with any LLM (Claude, GPT-4, local models)
- Can use Supabase RLS for security
- Schema is simple enough for good accuracy

**Example Flow:**
```
User: "What files did the agent read outside the project?"
→ Claude generates SQL (using schema context)
→ Execute query via Supabase
→ Claude formats results: "The agent read 5 files: /etc/ld.so.cache, ..."
```

### 2. **LangChain + SQL Agent**

**How it works:**
- Use LangChain's SQL Database Agent
- Agent can inspect schema and generate queries
- Supports iterative refinement

**Setup:**
```python
from langchain.agents import create_sql_agent
from langchain.sql_database import SQLDatabase
from langchain.llms import OpenAI

db = SQLDatabase.from_uri("postgresql://...")
agent = create_sql_agent(
    llm=OpenAI(temperature=0),
    db=db,
    verbose=True
)

result = agent.run("What files were read in the last session?")
```

**Pros:**
- Out-of-the-box solution
- Can handle complex multi-step queries
- Supports tool use

**Cons:**
- Adds dependency
- Can be overkill for simple queries

### 3. **Wren AI (Open Source)**

**How it works:**
- Self-hosted text-to-SQL platform
- Web UI for asking questions
- Generates SQL + visualizations

**Pros:**
- Open source
- Nice UI
- Can cache common queries

**Cons:**
- Requires separate service
- More complex setup

### 4. **Custom Prompt Engineering (Lightest Weight)**

**Schema Context Prompt:**
```
You are a SQL expert helping analyze syscall traces.

Database Schema:
- sessions: id (uuid), timestamp, program, working_dir, ...
- syscalls: id, session_id (fk), timestamp, syscall_name, category, raw_args (jsonb), raw_return

For the question "{user_question}", generate a PostgreSQL query.
Return ONLY valid SQL, no explanations.
```

**Pros:**
- Minimal code
- Works with any LLM
- Easy to customize

**Cons:**
- Need to handle result formatting separately
- May need retry logic for complex queries

---

## Recommended Approach for Demo

### Simple Implementation (Best for Demo)

```python
import anthropic
import json
from supabase import create_client

# Schema definition for Claude
SCHEMA_CONTEXT = """
Database Schema for Syscall Traces:

Table: sessions
- id: UUID (primary key)
- timestamp: timestamptz
- program: text (e.g., "python3")
- working_dir: text
- ...

Table: syscalls
- id: bigserial (primary key)
- session_id: UUID (references sessions.id)
- timestamp: text
- syscall_name: text (e.g., "openat", "socket")
- category: text (Process|File|Network|Unknown)
- raw_args: jsonb array
- raw_return: text

Common Patterns:
- Latest session: SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1
- File paths: raw_args->>1 (second element)
- Network addresses: Look for 'inet_addr' or 'AF_INET' in raw_args
- Errors: raw_return LIKE '%-1%'
"""

def ask_forensic_question(question: str):
    # 1. Get SQL from Claude
    client = anthropic.Anthropic()
    response = client.messages.create(
        model="claude-3-5-sonnet-20241022",
        max_tokens=1024,
        messages=[{
            "role": "user",
            "content": f"{SCHEMA_CONTEXT}\n\nGenerate a PostgreSQL query for: {question}\n\nReturn only the SQL query."
        }]
    )

    sql_query = response.content[0].text.strip()

    # 2. Execute via Supabase
    supabase = create_client(SUPABASE_URL, SUPABASE_KEY)
    result = supabase.rpc('execute_sql', {'query': sql_query}).execute()

    # 3. Format results with Claude
    response = client.messages.create(
        model="claude-3-5-sonnet-20241022",
        max_tokens=2048,
        messages=[{
            "role": "user",
            "content": f"Question: {question}\n\nQuery results:\n{json.dumps(result.data, indent=2)}\n\nProvide a natural language answer."
        }]
    )

    return response.content[0].text

# Usage
answer = ask_forensic_question("What files were read in the last session?")
print(answer)
```

---

## Migration Path

### Phase 1: Basic Schema (Now)
- `sessions` and `syscalls` tables
- Store raw_args as JSONB
- Manual/LLM-generated queries

### Phase 2: Enhanced Parsing (Later)
Add parsed fields to `syscalls`:
```sql
ALTER TABLE syscalls ADD COLUMN parsed_data JSONB;
-- Store type-specific parsed data (OpenAtSyscall, SocketSyscall, etc.)
```

### Phase 3: Specialized Tables (Future)
Create category-specific tables:
```sql
CREATE TABLE network_syscalls (
    syscall_id BIGINT REFERENCES syscalls(id),
    domain TEXT,
    socket_type TEXT,
    protocol TEXT,
    address TEXT,
    port INTEGER,
    ...
);
```

---

## Complete Data Flow

```
mini-capsule run <program>
    ↓
1. Create session record in DB (get UUID)
    ↓
2. Stream trace to 3 local files:
   - raw_trace.txt
   - structured_syscalls.jsonl
   - failed_parse_raw.txt
    ↓
3. After trace completes:
   a. Upload raw_trace.txt → Supabase Storage
   b. Upload failed_parse_raw.txt → Supabase Storage
   c. Batch insert syscalls → Supabase DB
   d. Update session with file paths + counts
    ↓
4. Query via natural language:
   User: "What files were accessed?"
   Claude: Generates SQL → Execute → Format results
   + Option to download raw_trace.txt for deep dive
```

## Benefits of This Architecture

### Separation of Concerns
- **Structured data (syscalls):** In DB for fast querying
- **Raw logs:** In object storage for debugging/audit
- **Best of both worlds:** Query structured, reference raw when needed

### Cost Efficient
- Don't pay for DB storage of large text files
- Object storage is much cheaper (~$0.021/GB vs Postgres storage)
- Only load raw files when actually needed

### Forensic Flexibility
```python
# Quick query: "Show me network syscalls"
result = supabase.table("syscalls").select("*").eq("category", "Network").execute()

# Deep dive: "I need to see the exact raw output"
raw_trace = supabase.storage.from_("trace-files").download(f"{session_id}/raw_trace.txt")

# Hybrid: "Show me the context around this syscall"
# Query for syscall timestamp, then grep raw_trace.txt
```

---

## Summary

**Recommended Setup:**
1. **Database:** Supabase PostgreSQL (already in your stack)
2. **Schema:** Simple 2-table design (sessions + syscalls)
3. **Object Storage:** Supabase Storage for raw files
4. **NL Interface:** Claude + custom prompt (lightest weight)
5. **Storage:** JSONB for raw_args (flexible, queryable)

**For Demo Queries:**
- Pre-write 3-5 example SQL queries as templates
- Use Claude to generate variations based on user questions
- Execute via Supabase client
- Format results with Claude
- Provide signed URLs to raw files when needed

This gives you:
✅ Simple schema that matches your data model
✅ Natural language interface without extra infrastructure
✅ Flexibility to add parsed fields later
✅ Fast queries with proper indexes
✅ Raw trace files always available for audit
✅ Cost-efficient storage (DB for structure, object storage for raw)
✅ Easy to extend for new syscall types

**Storage Costs (Example):**
- 1000 sessions/day × 500KB avg trace file = 500MB/day = 15GB/month
- Supabase Storage: ~$0.021/GB = ~$0.32/month
- Much cheaper than storing in DB or local filesystem
