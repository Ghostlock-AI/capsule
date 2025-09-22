"""
LangChain tools for search and shell execution.
"""

import os
import sys
import warnings

from langchain_community.tools import ShellTool
from langchain_community.tools.tavily_search import TavilySearchResults

# Suppress shell tool warnings
warnings.filterwarnings("ignore", message="The shell tool has no safeguards by default")

# Validate environment setup
if not os.getenv('TAVILY_API_KEY'):
    print("❌ Error: TAVILY_API_KEY environment variable is required.")
    print("   Please set it in your .env file or environment.")
    print("   Get your API key from: https://app.tavily.com/")
    sys.exit(1)

# Create tool instances
try:
    # Internet search with more results for comprehensive research
    search_tool = TavilySearchResults(
        max_results=5,
        search_depth="advanced",
        include_answer=True,
        include_raw_content=False,
        include_images=False
    )
    # Test the API key with a simple search
    test_result = search_tool.invoke({"query": "test"})
    print("✅ Tavily API key validated successfully")
except Exception as e:
    print(f"❌ Error: Tavily API key validation failed: {e}")
    print("   Please check your TAVILY_API_KEY is correct.")
    print("   Get your API key from: https://app.tavily.com/")
    sys.exit(1)

shell_tool = ShellTool()

# List of available tools
tools = [search_tool, shell_tool]
