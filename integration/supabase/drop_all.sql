-- Drop All Existing Schema
-- WARNING: This will delete ALL data in the capsule database
-- Run this ONLY if you want to completely reset the database

-- Drop existing tables (if any)
DROP TABLE IF EXISTS syscall_events CASCADE;
DROP TABLE IF EXISTS actions CASCADE;
DROP TABLE IF EXISTS runs CASCADE;
DROP TABLE IF EXISTS syscalls CASCADE;
DROP TABLE IF EXISTS sessions CASCADE;

-- Drop existing views
DROP VIEW IF EXISTS recent_runs CASCADE;

-- Drop existing functions
DROP FUNCTION IF EXISTS get_latest_run();
DROP FUNCTION IF EXISTS update_run_stats(UUID);

-- Drop storage bucket policies (if any)
DROP POLICY IF EXISTS "Service role can upload trace files" ON storage.objects;
DROP POLICY IF EXISTS "Service role can read trace files" ON storage.objects;
DROP POLICY IF EXISTS "Service role can update trace files" ON storage.objects;
DROP POLICY IF EXISTS "Service role can delete trace files" ON storage.objects;
DROP POLICY IF EXISTS "Authenticated users can upload trace files" ON storage.objects;
DROP POLICY IF EXISTS "Authenticated users can read trace files" ON storage.objects;

-- Drop RLS policies
DROP POLICY IF EXISTS "Service role can manage sessions" ON sessions;
DROP POLICY IF EXISTS "Service role can manage syscalls" ON syscalls;

-- Note: Storage buckets cannot be dropped via SQL
-- You need to delete them manually from Supabase dashboard: Storage → trace-files → Delete
-- Or recreate them with the new schema.sql

SELECT 'All tables, views, functions, and policies dropped successfully!' as status;
