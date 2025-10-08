# Remote Supabase Quick Start

Configure mini-capsule to send data to a remote Supabase instance in just a few steps.

## TL;DR

```bash
# 1. Create Supabase project at https://supabase.com
# 2. Run the schema SQL in your project
# 3. Configure minic
minic supa-config

# 4. Transfer your sessions
minic transfer --all
```

## What You Need

From your Supabase project (Settings → API):
- **Project URL**: `https://xxxxx.supabase.co`
- **service_role key**: The long JWT token (starts with `eyJ...`)

## Files

- **`remote_setup.sql`** - SQL schema to run in your Supabase project
- **`REMOTE_SETUP_GUIDE.md`** - Detailed step-by-step guide
- **`schema.sql`** - Same schema (works for both local and remote)

## The Flow

### 1. Setup Remote Supabase

```bash
# Go to https://supabase.com → New Project
# Copy the SQL from remote_setup.sql
# Run it in SQL Editor
```

### 2. Configure Connection

```bash
minic supa-config
```

This will:
- Prompt for your Supabase URL and keys
- Test the connection
- Save config to `~/.capsule/config.toml`
- Verify the schema is set up correctly

### 3. Transfer Data

```bash
# Transfer all sessions
minic transfer --all

# Or transfer a specific session
minic transfer --session 20251002_223525
```

### 4. (Optional) Query with AI

```bash
# Setup Anthropic API key
minic ai-setup

# Query your data
minic query "show me the last session"
```

## What Gets Configured

Your `~/.capsule/config.toml` will contain:

```toml
[supabase]
url = "https://xxxxx.supabase.co"
service_key = "eyJ..."
anon_key = "eyJ..."
enabled = true

[transfer]
auto_transfer = false
batch_size = 100
```

## Local vs Remote

The same schema works for both:
- **Local**: Docker container at `http://supabase-kong:8000`
- **Remote**: Your Supabase project at `https://xxxxx.supabase.co`

Just run `minic supa-config` to switch between them!

## Viewing Your Data

In Supabase Dashboard:
1. Go to **Table Editor**
2. View `sessions` and `syscalls` tables
3. Use **SQL Editor** for custom queries

## Need Help?

See **REMOTE_SETUP_GUIDE.md** for:
- Detailed setup instructions
- Troubleshooting steps
- Security best practices
- Advanced configuration
