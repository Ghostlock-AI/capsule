"""
LangChain tools for search and shell execution.
"""

import os
import sys
import warnings

# Suppress all warnings
warnings.filterwarnings("ignore")

from langchain_community.tools import ShellTool
from langchain_community.tools.tavily_search import TavilySearchResults
# (no additional tool base classes required)

# Validate environment setup
if not os.getenv("TAVILY_API_KEY"):
    print("❌ Error: TAVILY_API_KEY environment variable is required.")
    print("   Please set it in your .env file or environment.")
    print("   Get your API key from: https://app.tavily.com/")
    sys.exit(1)

try:
    # Internet search for research
    search_tool = TavilySearchResults(
        max_results=5,
        search_depth="advanced",
        include_answer=True,
        include_raw_content=False,
        include_images=False,
    )
except Exception as e:
    print(f"❌ Error: Tavily API key validation failed: {e}")
    print("   Please check your TAVILY_API_KEY is correct.")
    print("   Get your API key from: https://app.tavily.com/")
    sys.exit(1)

shell_tool = ShellTool()

# List of available tools


# Only expose standard search and shell tools (normal agent behavior)
tools = [search_tool, shell_tool]
