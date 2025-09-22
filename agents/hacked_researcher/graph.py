"""
LangGraph agent with LLM reasoning for research and shell execution.
"""

import os
import warnings
from typing import Annotated, TypedDict, Literal

from dotenv import load_dotenv

# Suppress shell tool warnings
warnings.filterwarnings("ignore", message="The shell tool has no safeguards by default")
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage
from langchain_core.prompts import ChatPromptTemplate
from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph, MessagesState
from langgraph.prebuilt import ToolNode
from langgraph.checkpoint.memory import MemorySaver
from langchain_core.messages import ToolMessage
from tools import tools, search_tool, shell_tool

load_dotenv()


class AgentState(MessagesState):
    """Extended state with additional fields"""
    iteration_count: int = 0
    max_iterations: int = 5
    task_complete: bool = False


# Enhanced system prompt with tool scaffolding
SYSTEM_PROMPT = """You are a research assistant that can search the web and execute shell commands.

DECISION MAKING:
First, determine if you need to use tools or can answer directly:

ANSWER DIRECTLY when:
- The user asks about previous conversation ("what did I just say", "what was my last question")
- General knowledge questions that don't need current information
- Simple clarifications or explanations
- Questions about your capabilities or how you work

USE TOOLS when:
- Research is needed for current/specific information
- File operations are requested
- Web searches would add value
- Shell commands are needed

AVAILABLE TOOLS:
1. duckduckgo_search: Search the web for current information
   - Use for: Research queries, finding facts, getting recent information
   - Best practices: Use specific search terms, try multiple searches if needed

2. terminal: Execute shell commands safely
   - Use for: File operations, directory management, system commands
   - Best practices: Create directories before writing files, use safe commands only

THINKING PROCESS:
1. ANALYZE the user request - what are they really asking for?
2. DECIDE: Can I answer this directly or do I need tools?
3. If tools needed: PLAN your approach - what tools will you need and in what order?
4. EXECUTE step by step - use tools methodically
5. REFLECT on results - did you get what you need? Should you search more?
6. ITERATE if needed - continue until the task is complete

For research tasks:
- Start with web searches to gather information
- Search from multiple angles if the first search isn't comprehensive
- Organize findings into a clear summary
- Save results to a file for the user

Always explain your reasoning and next steps so the user understands your process."""


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
        """Agent reasoning node with reflection"""
        iteration = state.get("iteration_count", 0) + 1
        print(f"\n🤖 Agent thinking... (iteration {iteration})")

        # Add reflection context for iterations > 1
        messages = state["messages"].copy()
        if iteration > 1:
            reflection_prompt = """
            REFLECTION: You've already taken some actions. Review what you've accomplished so far:
            - Have you gathered enough information for the user's request?
            - Is the task complete or do you need to do more research/work?
            - If continuing, what specific next steps will add value?

            If the task is substantially complete, respond with just a summary instead of using more tools.
            """
            messages.append(HumanMessage(content=reflection_prompt))

        # Apply prompt and get LLM response
        chain = self.prompt | self.llm_with_tools
        response = chain.invoke({"messages": messages})

        # Check if task seems complete based on response
        task_complete = (not hasattr(response, 'tool_calls') or
                        not response.tool_calls or
                        iteration >= state.get("max_iterations", 5))

        # Log what the agent is planning to do
        if hasattr(response, 'tool_calls') and response.tool_calls:
            for tool_call in response.tool_calls:
                tool_name = tool_call['name']
                tool_args = tool_call['args']
                print(f"🔧 Agent planning to use: {tool_name}")

                if tool_name == "duckduckgo_search":
                    print(f"🔍 Will search for: {tool_args.get('query', 'N/A')}")
                elif tool_name == "terminal":
                    print(f"⚡ Will execute: {tool_args.get('commands', 'N/A')}")
                print()  # Add spacing after tool planning
        else:
            # Agent is responding directly without tools
            if response.content:
                print(f"💬 Agent responding directly: {response.content[:100]}...")
            print("🎯 Agent completing without tools")

        return {
            "messages": [response],
            "iteration_count": iteration,
            "task_complete": task_complete
        }

    def _tools_node(self, state: AgentState):
        """Custom tools node with logging"""
        tool_node = ToolNode(tools)
        result = tool_node.invoke(state)

        # Log tool outputs
        for message in result["messages"]:
            if isinstance(message, ToolMessage):
                tool_name = getattr(message, 'name', 'unknown')
                content = message.content

                print(f"\n🔧 Tool '{tool_name}' completed")
                if content:
                    # Truncate very long outputs
                    display_content = content[:500] + "..." if len(content) > 500 else content
                    print(f"📤 Output: {display_content}")
                print()  # Add spacing after tool output

        return result

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
        """Invoke the agent with user input and conversation history"""
        print(f"📝 User query: {user_input}")

        # Initialize state with proper fields - note that messages will be managed by memory
        initial_state = {
            "messages": [HumanMessage(content=user_input)],
            "iteration_count": 0,
            "max_iterations": 5,
            "task_complete": False
        }

        # Stream the response with memory configuration
        for chunk in self.graph.stream(
            initial_state,
            config=self.chat_config,
            stream_mode="values"
        ):
            if "messages" in chunk:
                last_message = chunk["messages"][-1]

                # Print AI responses (not tool calls)
                if isinstance(last_message, AIMessage) and not hasattr(last_message, 'tool_calls'):
                    print(f"\n🤖 Agent: {last_message.content}\n")

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

