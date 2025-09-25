"""
LangGraph agent with LLM reasoning for research and shell execution.
"""

import os
import warnings
from typing import Annotated, Any, Dict, List, Literal, TypedDict

from dotenv import load_dotenv

# Suppress shell tool warnings
warnings.filterwarnings("ignore", message="The shell tool has no safeguards by default")
from langchain_core.messages import (AIMessage, BaseMessage, HumanMessage,
                                     SystemMessage, ToolMessage)
from langchain_core.prompts import ChatPromptTemplate
from langchain_openai import ChatOpenAI
from langgraph.checkpoint.memory import MemorySaver
from langgraph.graph import END, START, StateGraph
from langgraph.graph.message import add_messages
from langgraph.prebuilt import ToolNode

from tools import search_tool, shell_tool, tools

load_dotenv()


class AgentState(TypedDict):
    """Extended state with comprehensive research tracking"""
    messages: Annotated[List[BaseMessage], add_messages]
    iteration_count: int
    max_iterations: int
    task_complete: bool
    research_phase: str
    search_topics_covered: List[str]
    sources_found: List[Dict[str, Any]]
    research_notes: str


# Pragmatic research-and-action agent prompt
SYSTEM_PROMPT = """You are a pragmatic research-and-action agent using the ReAct pattern.

Goal: Achieve the user’s objective efficiently by planning, then using the best tool (search or shell) at the right time. Prefer the most direct path over exhaustive research.

Tools:
- search: find information on the internet.
- shell: list/read/write files, run commands, run python, curl/wget, etc.

Policy:
1) Plan first: outline a short action plan with the minimum steps to succeed.
2) Choose tools:
   - Use search for facts, references, recent developments.
   - Use shell for local file operations, reading/writing code, executing scripts, or when the task clearly requires terminal actions.
3) Keep steps tight: avoid looping on searches once you have enough to proceed.
4) Verify as you go: after actions, quickly check results (e.g., list files, show snippets) to confirm progress.
5) Finish with a concise summary and, if applicable, brief citations.

Output discipline:
- When using tools, be precise in commands/queries.
- Minimize unnecessary searches. Act when action is clearly needed.
"""


class ResearchAgent:
    def __init__(self):
        # Initialize LLM
        self.llm = ChatOpenAI(model="gpt-4o-mini", temperature=0)

        # Bind tools to LLM
        self.llm_with_tools = self.llm.bind_tools(tools)

        # Create prompt template
        self.prompt = ChatPromptTemplate.from_messages([
            ("system", SYSTEM_PROMPT),
            ("placeholder", "{messages}")
        ])

        # Memory and config for conversation history
        self.memory = MemorySaver()
        self.chat_config = {"configurable": {"user_id": "1", "thread_id": "1"}}

        # Build graph
        self.graph = self._build_graph()

    def _build_graph(self):
        """Build the LangGraph workflow"""
        builder = StateGraph(AgentState)

        # Add nodes
        builder.add_node("agent", self._agent_node)
        builder.add_node("tools", self._tools_node)

        # Set entry point
        builder.add_edge(START, "agent")

        # Add conditional edges from agent
        builder.add_conditional_edges(
            "agent",
            self._route_tools,
            ["tools", END]
        )

        # Tools always go back to agent
        builder.add_edge("tools", "agent")

        return builder.compile(checkpointer=self.memory)

    def _agent_node(self, state: AgentState):
        """Agent reasoning node with balanced plan/act"""
        iteration = state.get("iteration_count", 0) + 1
        print(f"\n🤖 Agent thinking... (iteration {iteration})")

        # Keep messages as-is; rely on system prompt for planning/acting
        messages = state["messages"].copy()

        # Apply prompt and get LLM response
        chain = self.prompt | self.llm_with_tools
        response = chain.invoke({"messages": messages})

        # Check if task seems complete
        task_complete = (not hasattr(response, 'tool_calls') or not response.tool_calls)

        # Minimal logging (hide shell details)
        if not (hasattr(response, 'tool_calls') and response.tool_calls):
            print("🎯 Providing final answer")

        return {
            "messages": [response],
            "iteration_count": iteration,
            "task_complete": task_complete
        }

    def _tools_node(self, state: AgentState):
        """Enhanced tools node with research tracking"""
        tool_node = ToolNode(tools)
        result = tool_node.invoke(state)

        # Track research progress
        updated_state = dict(state)
        sources_found = updated_state.get("sources_found", [])
        search_topics = updated_state.get("search_topics_covered", [])

        # Track research progress and extract sources silently
        for message in result["messages"]:
            if isinstance(message, ToolMessage):
                tool_name = getattr(message, 'name', 'unknown')
                content = message.content

                if tool_name in ['tavily_search_results_json', 'tavily_search_results']:
                    # Extract search query and track topics
                    last_ai_message = None
                    for msg in reversed(state["messages"]):
                        if hasattr(msg, 'tool_calls') and msg.tool_calls:
                            last_ai_message = msg
                            break

                    if last_ai_message:
                        for tool_call in last_ai_message.tool_calls:
                            if tool_call['name'] in ['tavily_search_results_json', 'tavily_search_results']:
                                search_query = tool_call['args'].get('query', '')
                                if search_query and search_query not in search_topics:
                                    search_topics.append(search_query)

                    # Extract and count unique sources
                    if content:
                        import re
                        url_pattern = r'https?://[^\s\)]+(?:[^\s\)\.]+)'
                        urls = re.findall(url_pattern, content)
                        for url in urls:
                            if url not in [source.get('url', '') for source in sources_found]:
                                sources_found.append({"url": url, "query": search_query if 'search_query' in locals() else 'unknown'})

                    # (quiet)

        # Update state with research tracking
        updated_result = dict(result)
        updated_result.update({
            "sources_found": sources_found,
            "search_topics_covered": search_topics
        })

        return updated_result

    def _route_tools(self, state: AgentState) -> Literal["tools", "__end__"]:
        """Route to tools or end based on LLM response and iteration limits"""
        last_message = state["messages"][-1]
        iteration = state.get("iteration_count", 0)
        max_iterations = state.get("max_iterations", 5)

        # End if task is marked complete or max iterations reached
        if state.get("task_complete", False) or iteration >= max_iterations:
            if iteration >= max_iterations:
                print(f"\n🔄 Reached maximum iterations ({max_iterations}), completing task")
            return END

        # Check if LLM wants to use tools
        if hasattr(last_message, 'tool_calls') and last_message.tool_calls:
            return "tools"
        else:
            return END

    def invoke(self, user_input: str):
        """Invoke the comprehensive research agent"""
        print(f"📝 Research Query: {user_input}")
        print(f"🔬 Initiating comprehensive research process...")

        # Initialize comprehensive research state
        initial_state = {
            "messages": [HumanMessage(content=user_input)],
            "iteration_count": 0,
            "max_iterations": 10,
            "task_complete": False,
            "research_phase": "planning",
            "search_topics_covered": [],
            "sources_found": [],
            "research_notes": ""
        }

        # Stream the response with memory configuration
        all_chunks = list(self.graph.stream(
            initial_state,
            config=self.chat_config,
            stream_mode="values"
        ))

        # Find the final AI response from the last chunk
        if all_chunks:
            final_chunk = all_chunks[-1]
            if "messages" in final_chunk:
                # Look for the last AI message without tool calls
                for message in reversed(final_chunk["messages"]):
                    if isinstance(message, AIMessage):
                        # Check if it has no tool calls or empty tool calls
                        has_tools = hasattr(message, 'tool_calls') and message.tool_calls
                        if not has_tools:
                            print(f"\n🤖 Agent: {message.content}\n")
                            break

    def stream_response(self, user_input: str):
        """Stream tokens as they come in"""
        return self.graph.stream(
            input={"messages": [HumanMessage(content=user_input)]},
            config=self.chat_config,
            stream_mode="messages",
        )


def create_agent_graph():
    """Factory function to create the agent"""
    return ResearchAgent()
