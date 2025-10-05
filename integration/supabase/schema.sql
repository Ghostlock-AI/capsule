-- =============================================
-- Capsule Mini-Capsule Database Schema
-- Simplified schema for mini-capsule session transfer
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

-- =============================================
-- STORAGE BUCKET SETUP
-- =============================================

-- Create storage schema if it doesn't exist
CREATE SCHEMA IF NOT EXISTS storage;

-- Create buckets table if it doesn't exist (Supabase usually creates this automatically)
CREATE TABLE IF NOT EXISTS storage.buckets (
    id text PRIMARY KEY,
    name text NOT NULL,
    public boolean DEFAULT false,
    created_at timestamptz DEFAULT now(),
    updated_at timestamptz DEFAULT now()
);

-- Create objects table if it doesn't exist
CREATE TABLE IF NOT EXISTS storage.objects (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    bucket_id text REFERENCES storage.buckets(id),
    name text NOT NULL,
    owner uuid,
    created_at timestamptz DEFAULT now(),
    updated_at timestamptz DEFAULT now(),
    last_accessed_at timestamptz DEFAULT now(),
    metadata jsonb
);

-- Create storage bucket for trace files
INSERT INTO storage.buckets (id, name, public)
VALUES ('trace-files', 'trace-files', false)
ON CONFLICT (id) DO NOTHING;

-- Enable RLS on storage objects
ALTER TABLE storage.objects ENABLE ROW LEVEL SECURITY;

-- Set up RLS policies for storage
CREATE POLICY "Service role can upload trace files"
ON storage.objects FOR INSERT
TO service_role
WITH CHECK (bucket_id = 'trace-files');

CREATE POLICY "Service role can read trace files"
ON storage.objects FOR SELECT
TO service_role
USING (bucket_id = 'trace-files');

CREATE POLICY "Service role can update trace files"
ON storage.objects FOR UPDATE
TO service_role
USING (bucket_id = 'trace-files');

CREATE POLICY "Service role can delete trace files"
ON storage.objects FOR DELETE
TO service_role
USING (bucket_id = 'trace-files');

-- =============================================
-- HELPER VIEWS AND FUNCTIONS
-- =============================================

-- View for recent sessions
CREATE VIEW recent_sessions AS
SELECT
    s.id,
    s.timestamp,
    s.end_timestamp,
    s.program,
    s.working_dir,
    s.total_syscalls,
    s.parsed_syscalls,
    s.failed_parses,
    ROUND(100.0 * s.parsed_syscalls / NULLIF(s.total_syscalls, 0), 2) as parse_success_rate,
    s.transferred_at
FROM sessions s
ORDER BY s.timestamp DESC;

-- Function to get latest session
CREATE OR REPLACE FUNCTION get_latest_session()
RETURNS UUID AS $$
BEGIN
    RETURN (SELECT id FROM sessions ORDER BY timestamp DESC LIMIT 1);
END;
$$ LANGUAGE plpgsql;

SELECT 'Schema created successfully!' as status;
