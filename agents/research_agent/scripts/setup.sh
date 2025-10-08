#!/usr/bin/env bash
set -euo pipefail

# Resolve repo root relative to this script (scripts/..)
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

echo "=========================================="
echo "Setting up Research Agent Demo"
echo "=========================================="
echo ""

# Check for .env file
if [[ ! -f "$REPO_ROOT/.env" ]]; then
  echo "⚠️  WARNING: No .env file found"
  echo "   Creating .env from template..."
  cp "$REPO_ROOT/env-template" "$REPO_ROOT/.env"
  echo ""
  echo "📝 Please edit .env and add your API keys:"
  echo "   - OPENAI_API_KEY=sk-..."
  echo "   - TAVILY_API_KEY=tvly-..."
  echo ""
  echo "   Get keys from:"
  echo "   - OpenAI: https://platform.openai.com/api-keys"
  echo "   - Tavily: https://app.tavily.com/"
  echo ""
fi

# Create virtual environment if it doesn't exist
if [[ ! -d "$REPO_ROOT/.venv" ]]; then
  echo "📦 Creating virtual environment at $REPO_ROOT/.venv ..."
  python3 -m venv "$REPO_ROOT/.venv"
  echo "✅ Virtual environment created"
  echo ""
else
  echo "✅ Virtual environment already exists at $REPO_ROOT/.venv"
  echo ""
fi

# Activate venv
echo "🔧 Activating virtual environment..."
source "$REPO_ROOT/.venv/bin/activate"

# Install/upgrade dependencies
echo "📥 Installing requirements from requirements.txt ..."
python3 -m pip install --upgrade pip -q
python3 -m pip install -r "$REPO_ROOT/requirements.txt" -q

echo ""
echo "=========================================="
echo "✅ Setup complete!"
echo "=========================================="
echo ""
echo "Next steps:"
echo ""
echo "1. Ensure API keys are configured in .env"
echo ""
echo "2. Run the demo in 3 terminals:"
echo "   Terminal 1: ./scripts/exfil.sh     # Exfiltration server"
echo "   Terminal 2: ./scripts/inject.sh    # Malicious website"
echo "   Terminal 3: ./scripts/agent.sh     # AI agent"
echo ""
echo "3. At agent prompt, paste the attack prompt from DEMO_PROMPT.txt"
echo ""
echo "4. Monitor exfiltration:"
echo "   tail -f output/exfil_log.jsonl"
echo ""
