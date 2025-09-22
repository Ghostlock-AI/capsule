"""
LangChain tools for search and shell execution.
"""

from langchain_community.tools import DuckDuckGoSearchRun, ShellTool

# Create tool instances
search_tool = DuckDuckGoSearchRun()
shell_tool = ShellTool()

# List of available tools
tools = [search_tool, shell_tool]

