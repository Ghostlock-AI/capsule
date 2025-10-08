# Remote Supabase Setup Guide

This guide will help you configure `minic` to transfer your capsule sessions to a remote Supabase instance instead of the local docker container.

## Prerequisites

1. A Supabase account (free tier works great!)
2. `minic` installed on your system
3. Sessions you want to transfer

## Step 1: Create a Supabase Project

1. Go to [https://supabase.com](https://supabase.com) and sign in/sign up
2. Click "New Project"
3. Choose your organization
4. Give your project a name (e.g., "capsule-traces")
5. Set a database password (save this!)
6. Choose a region close to you
7. Click "Create new project" and wait for setup to complete (~2 minutes)

## Step 2: Set Up the Database Schema

1. In your Supabase project dashboard, go to the **SQL Editor** (left sidebar)
2. Click "New Query"
3. Copy the entire contents of `integration/supabase/remote_setup.sql`
4. Paste it into the SQL editor
5. Click "Run" to execute the schema

This will create:
- `sessions` table - stores session metadata
- `syscalls` table - stores all syscall events
- Indexes for efficient querying
- Row Level Security policies
- Helper views and functions

## Step 3: Get Your Supabase Credentials

You'll need two pieces of information:

### Project URL
1. Go to **Settings** → **API** in the left sidebar
2. Under "Project URL", copy the URL (e.g., `https://xxxxx.supabase.co`)

### Service Role Key
1. Still in **Settings** → **API**
2. Under "Project API keys", find **service_role**
3. Click the eye icon to reveal it
4. Copy the entire key (starts with `eyJ...`)

⚠️ **Important**: Keep your service_role key secret! It has full access to your database.

### Anon Key (Optional)
- Also in **Settings** → **API** under "Project API keys"
- You can use this for read-only operations
- For `minic`, we'll primarily use the service_role key

## Step 4: Configure `minic`

Run the interactive configuration command:

```bash
minic supa-config
```

You'll be prompted for:

1. **Supabase project URL** - Paste the URL from Step 3
2. **Service role key** - Paste the service_role key from Step 3
3. **Anon key** - (Optional) Press Enter to skip, it will use service_role key

The command will:
- Test the connection to your Supabase
- Verify the schema is set up correctly
- Save the configuration to `~/.capsule/config.toml`

Example session:
```
🔐 Remote Supabase Configuration

Before starting, make sure you've:
  1. Created a Supabase project at https://supabase.com
  2. Run the SQL schema from integration/supabase/remote_setup.sql
  3. Have your project URL and service_role key ready

You will be asked for:
  • Project URL (Settings → API → Project URL)
  • service_role key (Settings → API → Project API keys → service_role)

Enter your Supabase project URL (e.g., https://xxxxx.supabase.co): https://abc123.supabase.co

Enter your Supabase service_role key: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...

Enter your Supabase anon key (or press Enter to use service_role key):

🔍 Testing connection to Supabase...
✓ Connection successful!

✓ Configuration saved to ~/.capsule/config.toml

📦 Next steps:
  1. Run: minic trace <your-program>
  2. Transfer sessions: minic transfer --all
  3. Query your data: minic query "show me the last session"

💡 Tip: Run 'minic ai-setup' to configure Claude AI querying
```

## Step 5: Transfer Your Sessions

Now you can transfer your local sessions to the remote Supabase:

```bash
# Transfer all sessions
minic transfer --all

# Transfer a specific session
minic transfer --session 20251002_223525

# Force re-transfer even if already transferred
minic transfer --all --force
```

## Step 6: Query Your Data (Optional)

If you want to use natural language querying with Claude:

```bash
# Configure your Anthropic API key
minic ai-setup

# Query your data
minic query "show me the last session"
minic query "what network syscalls happened today"
minic query "which programs made the most file operations"
```

## Configuration File

Your config will be saved at `~/.capsule/config.toml`:

```toml
# Capsule Configuration File
# Remote Supabase Configuration

[supabase]
url = "https://xxxxx.supabase.co"
service_key = "eyJ..."
anon_key = "eyJ..."
enabled = true

[transfer]
auto_transfer = false
batch_size = 100

# Optional: Configure AI querying
[ai]
anthropic_api_key = "sk-ant-..."
```

## Viewing Your Data in Supabase

1. Go to your Supabase project dashboard
2. Click **Table Editor** in the left sidebar
3. You'll see:
   - `sessions` - All your traced sessions
   - `syscalls` - All syscall events
   - `recent_sessions` - A view of recent sessions with statistics

## Troubleshooting

### Connection Failed

If `minic supa-config` reports a connection error:

1. **Verify your URL and keys are correct**
   - URL should start with `https://`
   - Service key should be the full JWT token

2. **Check the schema is installed**
   - Go to SQL Editor in Supabase
   - Run: `SELECT * FROM sessions LIMIT 1;`
   - Should return an empty result (not an error)

3. **Verify RLS policies**
   - Go to **Authentication** → **Policies**
   - Ensure the service_role policies exist for both tables

### Transfer Fails

If `minic transfer --all` fails:

1. **Check your config file**
   ```bash
   cat ~/.capsule/config.toml
   ```

2. **Test connection manually**
   ```bash
   minic supa-config  # Re-run configuration
   ```

3. **Check Supabase logs**
   - Go to **Logs** → **Postgres Logs** in your project dashboard
   - Look for error messages

### Schema Differences

If you're upgrading from local to remote:

1. The schema is identical between local and remote
2. Both use the same `remote_setup.sql` file
3. Your existing `minic` commands work the same way

## Security Best Practices

1. **Never commit your service_role key to git**
   - The config file contains secrets
   - Add `~/.capsule/config.toml` to `.gitignore` if you're tracking it

2. **Use Row Level Security (RLS)**
   - The schema includes RLS policies by default
   - Service role bypasses RLS for full access

3. **Rotate keys if compromised**
   - Go to **Settings** → **API**
   - Click "Roll JWT Secret" to generate new keys
   - Re-run `minic supa-config` with new keys

## Next Steps

- **Trace a program**: `minic trace python3 script.py`
- **Transfer sessions**: `minic transfer --all`
- **Query with AI**: `minic query "what did this program do"`
- **View in Supabase**: Check the Table Editor in your dashboard

## Support

If you encounter issues:

1. Check this guide for troubleshooting steps
2. Review the Supabase project logs
3. Verify the schema matches `remote_setup.sql`
4. Ensure you're using the latest version of `minic`
