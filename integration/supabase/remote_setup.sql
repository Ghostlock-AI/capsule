-- =============================================
-- Capsule Remote Supabase Setup
-- Run this SQL in your remote Supabase SQL Editor
-- =============================================

-- =============================================
-- TABLES
-- =============================================

-- Sessions table
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
    raw_trace_path TEXT,
    failed_parse_path TEXT,

    -- Statistics
    total_syscalls INTEGER DEFAULT 0,
    parsed_syscalls INTEGER DEFAULT 0,
    failed_parses INTEGER DEFAULT 0,

    -- Metadata
    transferred_at TIMESTAMPTZ DEFAULT NOW(),
    local_session_dir TEXT,  -- Original local path for reference

    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Syscall category enum type
CREATE TYPE syscall_category AS ENUM ('Process', 'File', 'Network', 'Unknown');

-- Syscalls table
CREATE TABLE syscalls (
    id BIGSERIAL PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    timestamp TEXT NOT NULL,
    pid INTEGER,
    syscall_number INTEGER NOT NULL,
    syscall_name TEXT NOT NULL,
    raw_args JSONB NOT NULL,
    raw_return TEXT NOT NULL,
    category syscall_category NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- =============================================
-- INDEXES
-- =============================================

-- Sessions indexes
CREATE INDEX idx_sessions_timestamp ON sessions(timestamp DESC);
CREATE INDEX idx_sessions_program ON sessions(program);
CREATE INDEX idx_sessions_transferred ON sessions(transferred_at DESC);

-- Syscalls indexes
CREATE INDEX idx_syscalls_session_id ON syscalls(session_id);
CREATE INDEX idx_syscalls_category ON syscalls(category);
CREATE INDEX idx_syscalls_name ON syscalls(syscall_name);
CREATE INDEX idx_syscalls_session_category ON syscalls(session_id, category);
CREATE INDEX idx_syscalls_raw_args ON syscalls USING GIN (raw_args);

-- =============================================
-- ROW LEVEL SECURITY (RLS)
-- =============================================

-- Enable RLS
ALTER TABLE sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE syscalls ENABLE ROW LEVEL SECURITY;

-- Allow service role to do everything (for our server-side transfers)
CREATE POLICY "Service role can manage sessions"
ON sessions FOR ALL
TO service_role
USING (true)
WITH CHECK (true);

CREATE POLICY "Service role can manage syscalls"
ON syscalls FOR ALL
TO service_role
USING (true)
WITH CHECK (true);
