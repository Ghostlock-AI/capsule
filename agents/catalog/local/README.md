# local agent

simple local research agent meant to test
capsules networking monitoring

### setup

```bash
# create .env file
touch .env

# open env file
vim .env

# in the .env file paste openai api key
OPENAI_API_KEY={your_key}

# optional: configure Base Tool endpoint
# If set, the agent will augment web results with Base Tool hits
# BASE_TOOL_URL can be a GET or POST endpoint (see below)
# BASE_TOOL_METHOD can be GET or POST (default GET)
# BASE_TOOL_TOKEN is optional bearer/API token
# BASE_TOOL_TIMEOUT seconds (default 10)
# Example:
# BASE_TOOL_URL=http://localhost:8000/search
# BASE_TOOL_METHOD=GET
# BASE_TOOL_TOKEN=your_service_token

# save and exit
:x
```

### running

```bash
# classic python setup
source .venv/bin/activate
pip install -r requirements.txt
python3 agent.py
```

### base tool integration

If `BASE_TOOL_URL` is set in your environment, the agent calls it for each
query and merges the results with DuckDuckGo snippets.

Expected response shapes (best-effort parsing):
- `{ "results": [ { "title", "url", "snippet" }, ... ] }`
- `[ { "title", "url", "text" }, ... ]`
- `{ "data": { "items": [ ... ] } }`

You can also force-enable/disable in code by passing `use_base_tool=True/False`
to `WebSearchAgent`.

### shell tool integration

Enable the shell tool to execute simple, safe commands when prompted:

```env
ENABLE_SHELL_TOOL=1
# optional: override the allowed executables (comma-separated)
SHELL_TOOL_ALLOWLIST=echo,ls,pwd,cat,head,tail,wc,grep,rg
# optional: time and size guards
SHELL_TOOL_TIMEOUT=5
SHELL_TOOL_MAX_OUTPUT=8000
```

Behavior:
- The agent routes: if you ask to run a command, it chooses the shell tool and executes a single allowed command (no pipes/redirection/multi‑commands). Otherwise it performs web search.
- Output is printed with exit code, stdout/stderr, and truncation notice when applicable.
