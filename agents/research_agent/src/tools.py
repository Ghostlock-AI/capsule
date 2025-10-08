"""
LangChain tools for search, web fetch, and shell execution.
"""

import os
import sys
import warnings

import requests

# Suppress all warnings
warnings.filterwarnings("ignore")

from langchain_community.tools import ShellTool
from langchain_community.tools.tavily_search import TavilySearchResults
from langchain_core.tools import tool

# Require Tavily for realistic search behavior
if not os.getenv("TAVILY_API_KEY"):
    print("❌ Error: TAVILY_API_KEY environment variable is required.")
    print("   Please set it in your .env file or environment.")
    print("   Get your API key from: https://app.tavily.com/")
    sys.exit(1)

try:
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


@tool("web_fetch")
def web_fetch(url: str) -> str:
    """Fetch and return the raw text content of a web resource.

    Use this tool to read pages discovered via search or to interact with HTTP
    services directly when investigating or gathering evidence. Accepts http/https
    URLs and returns the response body text (truncated for brevity).
    """
    try:
        if not (url.startswith("http://") or url.startswith("https://")):
            return "Error: URL must start with http:// or https://"
        resp = requests.get(url, timeout=10)
        resp.raise_for_status()
        text = resp.text or ""
        # Truncate to 20000 chars to ensure all injection payloads are visible
        return text[:20000]
    except Exception as e:
        return f"Error fetching URL: {e}"


shell_tool = ShellTool()

# Expose search, web fetch, and shell tools
tools = [search_tool, web_fetch, shell_tool]
