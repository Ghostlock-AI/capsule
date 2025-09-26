-- Capsule Database Schema
-- This file contains the complete schema for storing Capsule run data

-- =====================================================================
-- 1. CENTRAL RUNS TABLE
-- =====================================================================

CREATE TABLE runs (
    id UUID PRIMARY KEY,                    -- Same UUID from ~/.capsule/run/<uuid>
    command_line TEXT NOT NULL,             -- Complete command that was executed
    working_directory TEXT,                 -- CWD when run was started
    start_time TIMESTAMPTZ NOT NULL,        -- When run began
    end_time TIMESTAMPTZ,                   -- When run completed (NULL for running)
    exit_code INTEGER,                      -- Program exit code
    log_directory TEXT NOT NULL,            -- Path to ~/.capsule/logs/<timestamp-uuid>
    created_at TIMESTAMPTZ DEFAULT NOW(),
    
    -- Metadata for easier querying
    agent_type TEXT,                        -- "claude", "cursor", etc. (inferred from command)
    duration_ms BIGINT,                     -- Computed: end_time - start_time
    
    -- Summary statistics (computed from related tables)
    total_syscalls INTEGER DEFAULT 0,
    total_risk_events INTEGER DEFAULT 0,
    total_network_events INTEGER DEFAULT 0,
    total_file_operations INTEGER DEFAULT 0
);

-- Indexes for common queries
CREATE INDEX idx_runs_start_time ON runs(start_time DESC);
CREATE INDEX idx_runs_agent_type ON runs(agent_type);
CREATE INDEX idx_runs_command ON runs USING gin(to_tsvector('english', command_line));

-- =====================================================================
-- 2. SYSCALL EVENTS TABLE (Based on SyscallEvent struct)
-- =====================================================================

CREATE TABLE syscall_events (
    id BIGSERIAL PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    
    -- Core syscall data
    timestamp_us BIGINT NOT NULL,           -- Microseconds since tracer start
    pid INTEGER NOT NULL,
    syscall TEXT NOT NULL,
    syscall_number INTEGER,                 -- Syscall number from kernel
    args BIGINT[6],                         -- Fixed array of 6 args
    return_value BIGINT,
    raw_line TEXT,                          -- Original strace line
    
    -- Process context
    tid INTEGER,
    ppid INTEGER,
    exe_path TEXT,
    cwd TEXT,
    argv TEXT[],                            -- Command line arguments
    uid INTEGER,
    gid INTEGER,
    euid INTEGER,
    egid INTEGER,
    capabilities BIGINT,                    -- CapEff bitmap
    
    -- Resource context
    fd INTEGER,
    abs_path TEXT,
    fd_map JSONB,                          -- fd -> path/socket mapping
    resource_type TEXT,                     -- FILE, DIR, SOCKET, etc.
    operation TEXT,                         -- READ, WRITE, EXEC, etc.
    
    -- Operation details
    permission_bits INTEGER,               -- Octal mode
    byte_count BIGINT,
    latency_us BIGINT,
    
    -- Network context
    network_info JSONB,                    -- Family, protocol, addresses, ports
    
    -- Risk analysis
    risk_tags TEXT[],                      -- Array of risk flags
    high_level_kind TEXT,                  -- Bucket for aggregator
    
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_syscall_events_run_id ON syscall_events(run_id);
CREATE INDEX idx_syscall_events_timestamp ON syscall_events(run_id, timestamp_us);
CREATE INDEX idx_syscall_events_syscall ON syscall_events(syscall);
CREATE INDEX idx_syscall_events_risks ON syscall_events USING gin(risk_tags);
CREATE INDEX idx_syscall_events_operation ON syscall_events(operation);

-- =====================================================================
-- 3. ACTIONS TABLE (Based on Action struct)
-- =====================================================================

CREATE TABLE actions (
    id BIGSERIAL PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    
    -- Time range
    first_timestamp_us BIGINT NOT NULL,
    last_timestamp_us BIGINT NOT NULL,
    duration_us BIGINT GENERATED ALWAYS AS (last_timestamp_us - first_timestamp_us) STORED,
    
    -- Process context
    pids INTEGER[],                        -- Array of PIDs involved
    
    -- Action classification
    action_type TEXT NOT NULL,             -- FileRead, FileWrite, SocketConnect, etc.
    action_data JSONB NOT NULL,            -- Variant data (path, bytes, addr, etc.)
    
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_actions_run_id ON actions(run_id);
CREATE INDEX idx_actions_timestamp ON actions(run_id, first_timestamp_us);
CREATE INDEX idx_actions_type ON actions(action_type);
CREATE INDEX idx_actions_data ON actions USING gin(action_data);

-- =====================================================================
-- 4. VIEWS AND HELPER FUNCTIONS
-- =====================================================================

-- View for easy run browsing (most recent first)
CREATE VIEW recent_runs AS
SELECT 
    id,
    command_line,
    start_time,
    duration_ms,
    agent_type,
    total_syscalls,
    total_risk_events,
    CASE 
        WHEN end_time IS NULL THEN 'RUNNING'
        WHEN exit_code = 0 THEN 'SUCCESS' 
        ELSE 'FAILED'
    END as status
FROM runs 
ORDER BY start_time DESC;

-- Function to get "latest" run easily  
CREATE OR REPLACE FUNCTION get_latest_run()
RETURNS UUID AS $$
BEGIN
    RETURN (SELECT id FROM runs ORDER BY start_time DESC LIMIT 1);
END;
$$ LANGUAGE plpgsql;

-- Function to update run statistics
CREATE OR REPLACE FUNCTION update_run_stats(run_uuid UUID)
RETURNS VOID AS $$
BEGIN
    UPDATE runs SET
        total_syscalls = (
            SELECT COUNT(*) FROM syscall_events WHERE run_id = run_uuid
        ),
        total_risk_events = (
            SELECT COUNT(*) FROM syscall_events 
            WHERE run_id = run_uuid AND array_length(risk_tags, 1) > 0
        ),
        total_network_events = (
            SELECT COUNT(*) FROM syscall_events 
            WHERE run_id = run_uuid AND network_info IS NOT NULL
        ),
        total_file_operations = (
            SELECT COUNT(*) FROM syscall_events 
            WHERE run_id = run_uuid AND operation IN ('READ', 'write', 'open', 'close')
        )
    WHERE id = run_uuid;
END;
$$ LANGUAGE plpgsql;

-- =====================================================================
-- 5. SAMPLE DATA FOR TESTING
-- =====================================================================

-- Insert a sample run for testing
INSERT INTO runs (
    id, 
    command_line, 
    working_directory, 
    start_time, 
    end_time, 
    exit_code, 
    log_directory,
    agent_type,
    duration_ms
) VALUES (
    '123e4567-e89b-12d3-a456-426614174000',
    'capsule run claude --edit main.py',
    '/home/user/project',
    NOW() - INTERVAL '1 hour',
    NOW() - INTERVAL '58 minutes',
    0,
    '/home/user/.capsule/logs/20250701T120000Z-123e4567',
    'claude',
    120000
);

-- Insert sample syscall events
INSERT INTO syscall_events (
    run_id,
    timestamp_us,
    pid,
    syscall,
    args,
    return_value,
    raw_line,
    exe_path,
    operation,
    resource_type,
    risk_tags
) VALUES 
(
    '123e4567-e89b-12d3-a456-426614174000',
    1000000,
    1234,
    'openat',
    ARRAY[4, 123456789, 0, 0, 0, 0],
    3,
    '1000000 1234 openat(AT_FDCWD, "main.py", O_RDWR) = 3',
    '/usr/bin/python3',
    'open',
    'file',
    ARRAY[]::TEXT[]
),
(
    '123e4567-e89b-12d3-a456-426614174000',
    2000000,
    1234,
    'connect',
    ARRAY[3, 123456789, 16, 0, 0, 0],
    0,
    '2000000 1234 connect(3, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("1.2.3.4")}, 16) = 0',
    '/usr/bin/python3',
    'connect',
    'socket',
    ARRAY['OUTBOUND_CONNECT']
);

-- Update statistics for the sample run
SELECT update_run_stats('123e4567-e89b-12d3-a456-426614174000');