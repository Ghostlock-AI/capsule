use anyhow::{Context, Result};
use comfy_table::{presets::UTF8_FULL, Cell, ContentArrangement, Table};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, Column, PgPool, Row, TypeInfo};

const MAX_DISPLAY_ROWS: usize = 50;
const MAX_CELL_WIDTH: usize = 80;

// =============================================
// ANTHROPIC API TYPES
// =============================================

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

// =============================================
// SCHEMA CONTEXT FOR LLM
// =============================================

const SCHEMA_CONTEXT: &str = r#"You are a PostgreSQL query generator for a syscall tracing database.

<schema>
-- Syscall category enum type (case-sensitive!)
CREATE TYPE syscall_category AS ENUM ('Process', 'File', 'Network', 'Unknown');

-- Sessions table: stores syscall tracing sessions
-- Each session = one program execution traced by mini-capsule
CREATE TABLE sessions (
    id UUID PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL,        -- Session start time (use for "last session" queries)
    end_timestamp TIMESTAMPTZ,             -- Session end time (NULL if still running)
    os TEXT NOT NULL,                      -- Operating system (e.g., 'Linux', 'Darwin')
    chipset TEXT NOT NULL,                 -- CPU architecture (e.g., 'x86_64', 'arm64')
    working_dir TEXT NOT NULL,             -- Directory where program was executed
    program TEXT NOT NULL,                 -- Executable name (e.g., 'python', 'curl', 'node')
    args TEXT NOT NULL,                    -- Command-line arguments
    total_syscalls INTEGER DEFAULT 0,     -- Total syscalls captured
    parsed_syscalls INTEGER DEFAULT 0,    -- Successfully parsed syscalls
    transferred_at TIMESTAMPTZ DEFAULT NOW()
);

-- Syscalls table: stores individual system calls
-- raw_args is JSONB containing syscall-specific data (filenames, addresses, etc.)
CREATE TABLE syscalls (
    id BIGSERIAL PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sessions(id),  -- Links to sessions table
    timestamp TEXT NOT NULL,               -- Syscall timestamp (string format from strace)
    pid INTEGER,                           -- Process ID that made the syscall
    syscall_number INTEGER NOT NULL,       -- Linux syscall number
    syscall_name TEXT NOT NULL,            -- Human-readable name (e.g., 'read', 'connect', 'fork')
    raw_args JSONB NOT NULL,               -- Syscall arguments as JSON (query with ->, ->>)
    raw_return TEXT NOT NULL,              -- Return value from syscall
    category syscall_category NOT NULL     -- High-level category
);

-- Key indexes (use these for optimization)
CREATE INDEX idx_sessions_timestamp ON sessions(timestamp DESC);  -- For "last session" queries
CREATE INDEX idx_syscalls_session_category ON syscalls(session_id, category);  -- For filtering by category
CREATE INDEX idx_syscalls_raw_args ON syscalls USING GIN (raw_args);  -- For JSONB queries
</schema>

<relationships>
sessions.id → syscalls.session_id (ONE-TO-MANY)
- One session has many syscalls
- Use JOIN when fetching syscalls with session info
- Common pattern: WHERE session_id = (SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1)
</relationships>

<jsonb_fields>
raw_args contains syscall-specific data stored as JSONB arrays (examples):

FILE SYSCALLS (openat, read, write):
- Structure: ["fd", "filename", "flags", ...]
- Example: ["AT_FDCWD", "/etc/hosts", "O_RDONLY"]
- Query filenames: raw_args->>1 (second element is typically the path)
- Pattern match: raw_args->>1 LIKE '/etc/%'

NETWORK SYSCALLS (connect, sendto, recvfrom, bind):
- Structure: ["socket_fd", "{address_struct}", "data/flags", ...]
- Example connect: ["4<TCP:[18898905]>", "{sa_family=AF_INET, inet_addr(\"192.168.1.1\"), sin_port=htons(443)}"]
- Example sendto: ["4<TCP:[..]>", "Hello from client!", "18", "0", "{sa_family=AF_INET, ...}"]
- Addresses appear in raw_args as formatted strings containing:
  - sa_family: AF_INET (IPv4), AF_INET6 (IPv6), AF_UNIX (local sockets)
  - inet_addr("IP"): The actual IP address
  - sin_port=htons(PORT): The port number
- Query for external servers: raw_args::text LIKE '%inet_addr%' (finds IP addresses)
- Query for specific IPs: raw_args::text LIKE '%192.168.%'
- Extract all network data: Convert raw_args to text and parse address structures

PROCESS SYSCALLS (execve, fork, clone):
- Structure: ["command", "[args]", "[env]", ...]
- Example: ["/bin/bash", "[\"script.sh\", \"arg1\"]", "[\"PATH=/usr/bin\", ...]"]

To query JSONB:
- Extract array element: raw_args->0 (JSON), raw_args->>0 (text)
- Pattern match text: raw_args::text LIKE '%pattern%'
- Check for addresses: raw_args::text LIKE '%inet_addr%'
- File paths (usually index 1): raw_args->>1 LIKE '/etc/%'
</jsonb_fields>

<common_patterns>
"Last session": WHERE session_id = (SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1)
"Recent sessions": WHERE timestamp > NOW() - INTERVAL '1 day'
"Unique values": SELECT DISTINCT column_name
"File operations": WHERE category = 'File'
"Network activity": WHERE category = 'Network'
"External servers": WHERE category = 'Network' AND raw_args::text LIKE '%inet_addr%'
"Network syscalls with data": WHERE category = 'Network' AND syscall_name IN ('connect', 'sendto', 'recvfrom', 'bind')
"Files accessed": WHERE category = 'File' AND raw_args->>1 IS NOT NULL
</common_patterns>

<examples>
<example>
<question>Show me the last session</question>
<sql>SELECT * FROM sessions ORDER BY timestamp DESC LIMIT 1</sql>
</example>

<example>
<question>What network syscalls happened in the last session?</question>
<sql>
SELECT s.syscall_name, s.raw_args, s.timestamp
FROM syscalls s
WHERE s.session_id = (SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1)
  AND s.category = 'Network'
</sql>
</example>

<example>
<question>What external addresses were networked with in the last session?</question>
<sql>
SELECT s.syscall_name, s.raw_args, s.timestamp
FROM syscalls s
WHERE s.session_id = (SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1)
  AND s.category = 'Network'
  AND s.raw_args::text LIKE '%inet_addr%'
ORDER BY s.timestamp
</sql>
</example>

<example>
<question>Display all external servers communicated with</question>
<sql>
SELECT
    s.syscall_name,
    s.raw_args,
    s.timestamp
FROM syscalls s
WHERE s.session_id = (SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1)
  AND s.category = 'Network'
  AND (s.syscall_name IN ('connect', 'sendto', 'recvfrom', 'bind')
       OR s.raw_args::text LIKE '%inet_addr%')
ORDER BY s.timestamp
</sql>
</example>

<example>
<question>Show unique external IP addresses from the last session</question>
<sql>
SELECT DISTINCT
    s.raw_args::text as network_data,
    s.syscall_name
FROM syscalls s
WHERE s.session_id = (SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1)
  AND s.category = 'Network'
  AND s.raw_args::text LIKE '%inet_addr%'
</sql>
</example>

<example>
<question>Show all file operations from yesterday</question>
<sql>
SELECT s.syscall_name, s.raw_args, s.timestamp
FROM syscalls s
JOIN sessions sess ON s.session_id = sess.id
WHERE sess.timestamp > NOW() - INTERVAL '1 day'
  AND s.category = 'File'
ORDER BY s.timestamp DESC
</sql>
</example>

<example>
<question>Which sessions had the most network activity?</question>
<sql>
SELECT sess.id, sess.program, sess.timestamp, COUNT(*) as network_count
FROM sessions sess
JOIN syscalls s ON sess.id = s.session_id
WHERE s.category = 'Network'
GROUP BY sess.id, sess.program, sess.timestamp
ORDER BY network_count DESC
LIMIT 10
</sql>
</example>

<example>
<question>What unique programs have been traced?</question>
<sql>
SELECT DISTINCT program FROM sessions ORDER BY program
</sql>
</example>

<example>
<question>Show syscalls that accessed /etc files</question>
<sql>
SELECT syscall_name, raw_args->>'filename' as filepath, timestamp
FROM syscalls
WHERE category = 'File'
  AND raw_args->>'filename' LIKE '/etc/%'
ORDER BY timestamp DESC
LIMIT 50
</sql>
</example>

<example>
<question>Sessions from the last 7 days</question>
<sql>
SELECT id, program, timestamp, total_syscalls
FROM sessions
WHERE timestamp > NOW() - INTERVAL '7 days'
ORDER BY timestamp DESC
</sql>
</example>

<example>
<question>Which programs made more than 1000 syscalls?</question>
<sql>
SELECT sess.program, COUNT(*) as syscall_count
FROM sessions sess
JOIN syscalls s ON sess.id = s.session_id
GROUP BY sess.program
HAVING COUNT(*) > 1000
ORDER BY syscall_count DESC
</sql>
</example>

<example>
<question>Top 10 most common syscalls in the last session</question>
<sql>
SELECT syscall_name, COUNT(*) as count
FROM syscalls
WHERE session_id = (SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1)
GROUP BY syscall_name
ORDER BY count DESC
LIMIT 10
</sql>
</example>

<example>
<question>What files were read in the last session?</question>
<sql>
SELECT syscall_name, raw_args, timestamp
FROM syscalls
WHERE session_id = (SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1)
  AND category = 'File'
  AND syscall_name IN ('read', 'openat', 'readlinkat')
ORDER BY timestamp
</sql>
</example>

<example>
<question>What files were written in the last session?</question>
<sql>
SELECT syscall_name, raw_args, timestamp
FROM syscalls
WHERE session_id = (SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1)
  AND category = 'File'
  AND syscall_name IN ('write', 'openat')
ORDER BY timestamp
</sql>
</example>
</examples>

<instructions>
1. Generate ONLY valid PostgreSQL SQL
2. Use exact table and column names from schema
3. For "last session": ORDER BY timestamp DESC LIMIT 1
4. Category values are ENUM: 'Process', 'File', 'Network', 'Unknown' (case-sensitive!)
5. raw_args is JSONB - use ->> for text, -> for JSON objects
6. Return ONLY the SQL query, no explanation or markdown
7. Do not include semicolon at the end
8. Use explicit column names (not SELECT *)
9. For "unique" or "different" values: use SELECT DISTINCT
10. For filtering aggregates: use HAVING (not WHERE)
11. Add LIMIT for large result sets
</instructions>"#;

// =============================================
// QUERY ENGINE
// =============================================

pub struct QueryEngine {
    pool: PgPool,
    http_client: Client,
    api_key: String,
}

impl QueryEngine {
    pub fn new(pool: PgPool, api_key: String) -> Self {
        Self {
            pool,
            http_client: Client::new(),
            api_key,
        }
    }

    /// Execute a natural language query and display results
    pub async fn run(&self, question: &str) -> Result<()> {
        // 1. Generate SQL
        println!("🤔 Thinking...");
        let sql = self.generate_sql(question).await?;
        println!("📝 Generated SQL:\n{}\n", sql);

        // 2. Validate SQL (safety check)
        self.validate_sql(&sql)?;

        // 3. Execute query
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .context("Failed to execute query")?;

        // 4. Display results
        self.display_table(rows)?;

        Ok(())
    }

    /// Generate SQL from natural language using Anthropic API
    async fn generate_sql(&self, question: &str) -> Result<String> {
        let prompt = format!(
            "{}\n\n<question>{}</question>\n\nSQL:",
            SCHEMA_CONTEXT, question
        );

        let request = AnthropicRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1024,
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let response = self
            .http_client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to call Anthropic API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error ({}): {}", status, error_text);
        }

        let api_response = response
            .json::<AnthropicResponse>()
            .await
            .context("Failed to parse Anthropic response")?;

        let sql = api_response
            .content
            .first()
            .map(|c| c.text.trim())
            .context("No content in Anthropic response")?
            .trim_matches('`')
            .trim_start_matches("sql")
            .trim()
            .to_string();

        Ok(sql)
    }

    /// Validate SQL for safety (only allow SELECT)
    fn validate_sql(&self, sql: &str) -> Result<()> {
        let sql_upper = sql.trim().to_uppercase();

        // Only allow SELECT queries
        if !sql_upper.starts_with("SELECT") {
            anyhow::bail!("Only SELECT queries are allowed");
        }

        // Block dangerous keywords
        let dangerous = [
            "DROP", "DELETE", "TRUNCATE", "INSERT", "UPDATE", "ALTER", "CREATE",
        ];
        for keyword in dangerous {
            if sql_upper.contains(keyword) {
                anyhow::bail!("Dangerous SQL keyword detected: {}", keyword);
            }
        }

        Ok(())
    }

    /// Display query results as a formatted table
    fn display_table(&self, rows: Vec<PgRow>) -> Result<()> {
        if rows.is_empty() {
            println!("No results found.");
            return Ok(());
        }

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_content_arrangement(ContentArrangement::Dynamic);

        // Add headers
        let columns = rows[0].columns();
        table.set_header(columns.iter().map(|c| c.name()).collect::<Vec<_>>());

        // Limit displayed rows
        let display_rows = &rows[..rows.len().min(MAX_DISPLAY_ROWS)];

        // Add data rows
        for row in display_rows {
            let cells: Vec<Cell> = columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let value_str = self.extract_cell_value(row, i, col.type_info().name());
                    Cell::new(value_str)
                })
                .collect();

            table.add_row(cells);
        }

        println!("{}", table);

        // Show pagination info
        if rows.len() > MAX_DISPLAY_ROWS {
            println!(
                "\n(Showing first {} of {} rows)",
                MAX_DISPLAY_ROWS,
                rows.len()
            );
        }

        Ok(())
    }

    /// Extract and format cell value based on PostgreSQL type
    fn extract_cell_value(&self, row: &PgRow, index: usize, type_name: &str) -> String {
        match type_name {
            "TEXT" | "VARCHAR" => row
                .try_get::<String, _>(index)
                .map(|v| Self::truncate_string(v))
                .unwrap_or_else(|_| "NULL".to_string()),

            "INT4" => row
                .try_get::<i32, _>(index)
                .map(|v| v.to_string())
                .unwrap_or_else(|_| "NULL".to_string()),

            "INT8" | "BIGINT" => row
                .try_get::<i64, _>(index)
                .map(|v| v.to_string())
                .unwrap_or_else(|_| "NULL".to_string()),

            "UUID" => row
                .try_get::<uuid::Uuid, _>(index)
                .map(|v| v.to_string())
                .unwrap_or_else(|_| "NULL".to_string()),

            "TIMESTAMPTZ" => row
                .try_get::<chrono::DateTime<chrono::Utc>, _>(index)
                .map(|v| v.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|_| "NULL".to_string()),

            "JSONB" | "JSON" => row
                .try_get::<serde_json::Value, _>(index)
                .map(|v| Self::truncate_string(v.to_string()))
                .unwrap_or_else(|_| "NULL".to_string()),

            // Handle custom enum types
            _ if type_name.to_uppercase() == "SYSCALL_CATEGORY" => row
                .try_get::<String, _>(index)
                .unwrap_or_else(|_| "NULL".to_string()),

            _ => format!("({})", type_name),
        }
    }

    /// Truncate long strings for display
    fn truncate_string(s: String) -> String {
        if s.len() > MAX_CELL_WIDTH {
            format!("{}...", &s[..MAX_CELL_WIDTH - 3])
        } else {
            s
        }
    }
}
