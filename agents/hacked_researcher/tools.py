"""
LangChain tools for search and shell execution.
"""

import warnings
from typing import Optional, Type
from langchain_core.callbacks import CallbackManagerForToolRun
from langchain_core.tools import BaseTool
from langchain_community.tools import ShellTool
from pydantic import BaseModel, Field

# Suppress shell tool warnings
warnings.filterwarnings("ignore", message="The shell tool has no safeguards by default")

try:
    from ddgs import DDGS
except ImportError:
    DDGS = None


class DuckDuckGoSearchInput(BaseModel):
    query: str = Field(description="search query to look up")


class DuckDuckGoSearchRun(BaseTool):
    """DuckDuckGo search using ddgs package"""

    name: str = "duckduckgo_search"
    description: str = (
        "A wrapper around DuckDuckGo Search. "
        "Useful for when you need to answer questions about current events. "
        "Input should be a search query."
    )
    args_schema: Type[BaseModel] = DuckDuckGoSearchInput

    def _run(
        self,
        query: str,
        run_manager: Optional[CallbackManagerForToolRun] = None,
    ) -> str:
        if not DDGS:
            return "DuckDuckGo search unavailable - ddgs not installed"

        try:
            ddgs = DDGS()
            results = ddgs.text(
                query,
                max_results=5,
                region='us-en',
                safesearch='moderate'
            )

            if not results:
                return f"No search results found for query: {query}"

            # Log the URLs being accessed
            print(f"🌐 Found {len(results)} search results:")
            for i, result in enumerate(results, 1):
                url = result.get('href', 'No URL')
                title = result.get('title', 'No title')
                print(f"   {i}. {title}")
                print(f"      🔗 {url}")
            print()  # Add spacing

            # Format results for LLM
            formatted_results = []
            for result in results:
                title = result.get('title', 'No title')
                body = result.get('body', 'No description')
                href = result.get('href', '')
                formatted_results.append(f"{title}: {body} (Source: {href})")

            return " | ".join(formatted_results)

        except Exception as e:
            return f"Search error: {str(e)}"


# Create tool instances
search_tool = DuckDuckGoSearchRun()
shell_tool = ShellTool()

# List of available tools
tools = [search_tool, shell_tool]

