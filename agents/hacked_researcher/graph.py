"""
LangGraph agent with LLM reasoning for research and shell execution.
"""

import os
import warnings
from typing import Annotated, TypedDict, Literal, List, Dict, Any

from dotenv import load_dotenv

# Suppress shell tool warnings
warnings.filterwarnings("ignore", message="The shell tool has no safeguards by default")
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage, BaseMessage
from langchain_core.prompts import ChatPromptTemplate
from langchain_openai import ChatOpenAI
from langgraph.graph import END, START, StateGraph
from langgraph.graph.message import add_messages
from langgraph.prebuilt import ToolNode
from langgraph.checkpoint.memory import MemorySaver
from langchain_core.messages import ToolMessage
from tools import tools, search_tool, shell_tool

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


# Comprehensive Research Agent System Prompt
SYSTEM_PROMPT = """You are an expert research agent designed to conduct thorough, multi-faceted research using the ReAct pattern.

MISSION: Provide comprehensive, well-researched answers by systematically gathering information from multiple sources, analyzing it, and presenting findings with proper citations.

## RESEARCH METHODOLOGY

### PHASE 1: RESEARCH PLANNING
THOUGHT: Before starting, analyze the query to determine:
- What are the key concepts I need to research?
- What different angles or perspectives should I explore?
- What types of sources would be most valuable?
- Should I break this into sub-topics?

### PHASE 2: SYSTEMATIC INFORMATION GATHERING
ACTION: Conduct multiple targeted searches covering:
1. **Primary search**: Main topic with specific terminology
2. **Contextual search**: Background, history, or foundational concepts
3. **Current research**: Recent developments, studies, or news
4. **Multiple perspectives**: Different viewpoints, criticisms, or debates
5. **Applications/implications**: Real-world uses, impacts, or consequences

### PHASE 3: KNOWLEDGE SYNTHESIS
OBSERVATION: After each search, assess:
- What new information did I gather?
- How does this relate to what I already know?
- What gaps remain in my understanding?
- Do I need to research any prerequisite concepts?

### PHASE 4: DOCUMENTATION & ORGANIZATION
ACTION: Use terminal to:
- Create organized research notes files
- Structure findings by topic/theme
- Maintain source lists with URLs
- Create summary documents

### PHASE 5: COMPREHENSIVE RESPONSE
THOUGHT: Synthesize all gathered information into:
- Clear, structured explanation of the topic
- Multiple perspectives where relevant
- Recent developments and current state
- Practical applications or implications
- Areas for further exploration
- Properly formatted citations

## SEARCH STRATEGY GUIDELINES

### FOR DIFFERENT TOPIC TYPES:
- **Scientific concepts**: "[concept] definition principles applications recent research"
- **Historical topics**: "[topic] history timeline significance impact analysis"
- **Literary works**: "[author] [work] analysis themes significance criticism"
- **Technical subjects**: "[topic] explanation how it works applications current developments"
- **Current events**: "[topic] news recent developments 2024 analysis"

### SEARCH DEPTH REQUIREMENTS:
- Minimum 3-5 different search angles per topic
- Search both foundational and recent information
- Look for authoritative sources (academic, institutional, expert)
- Cross-reference information across sources

### QUALITY INDICATORS:
- Search for peer-reviewed sources when possible
- Prioritize .edu, .org, and established institutions
- Look for recent publications (2020+) for current topics
- Seek multiple perspectives on controversial topics

## FILE ORGANIZATION SYSTEM

Create structured research files:
```
research_notes_[topic].md
├── Executive Summary
├── Key Concepts & Definitions
├── Historical Context
├── Current State
├── Multiple Perspectives
├── Applications & Implications
├── Future Directions
└── Sources & References
```

## CITATION FORMAT
Always include at end of response:

**Sources:**
1. [Title] - [URL] (Accessed: [Date])
2. [Title] - [URL] (Accessed: [Date])
...

## QUALITY CRITERIA
Before concluding research:
- ✓ Covered main aspects of the topic
- ✓ Included recent developments
- ✓ Found multiple perspectives
- ✓ Verified information across sources
- ✓ Organized findings logically
- ✓ Cited all sources properly

REMEMBER: The goal is not just to answer the question, but to provide a comprehensive understanding of the topic that could serve as a foundation for further learning or decision-making."""


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

        # Add comprehensive research reflection for iterations > 1
        messages = state["messages"].copy()
        if iteration > 1:
            phase = state.get("research_phase", "gathering")
            topics_covered = state.get("search_topics_covered", [])
            sources_count = len(state.get("sources_found", []))

            research_reflection = f"""
            COMPREHENSIVE RESEARCH REFLECTION:

            CURRENT PHASE: {phase}
            ITERATION: {iteration}
            TOPICS RESEARCHED: {topics_covered}
            SOURCES FOUND: {sources_count}

            THOUGHT: Evaluate my research progress:
            - Have I covered the main aspects of this topic from multiple angles?
            - Do I have sufficient sources ({sources_count}) for a comprehensive answer?
            - What key perspectives or recent developments might I be missing?
            - Should I search for more specific subtopics or move to synthesis?

            RESEARCH QUALITY CHECK:
            - ✓ Multiple search angles?
            - ✓ Recent developments included?
            - ✓ Different perspectives gathered?
            - ✓ Authoritative sources found?

            ACTION DECISION: Based on gaps identified, either:
            1. Continue targeted searches for missing information
            2. Create research documentation files
            3. Provide comprehensive final answer with citations
            """
            messages.append(HumanMessage(content=research_reflection))

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
                print(f"💬 Agent responding directly (preview): {response.content[:100]}...")
            print("🎯 Agent completing without tools")

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

        # Log tool outputs and extract sources
        for message in result["messages"]:
            if isinstance(message, ToolMessage):
                tool_name = getattr(message, 'name', 'unknown')
                content = message.content

                print(f"\n🔧 Tool '{tool_name}' completed")

                if tool_name == "duckduckgo_search" and content:
                    # Extract search query from the last agent message with tool calls
                    last_ai_message = None
                    for msg in reversed(state["messages"]):
                        if hasattr(msg, 'tool_calls') and msg.tool_calls:
                            last_ai_message = msg
                            break

                    if last_ai_message:
                        for tool_call in last_ai_message.tool_calls:
                            if tool_call['name'] == 'duckduckgo_search':
                                search_query = tool_call['args'].get('query', '')
                                if search_query and search_query not in search_topics:
                                    search_topics.append(search_query)
                                    print(f"📋 Added to research topics: {search_query}")

                    # Extract URLs from search results for source tracking
                    if "Source:" in content:
                        import re
                        url_pattern = r'https?://[^\s\)]+(?:[^\s\)\.]+)'
                        urls = re.findall(url_pattern, content)
                        for url in urls:
                            if url not in [source.get('url', '') for source in sources_found]:
                                sources_found.append({"url": url, "content": content[:200]})
                                print(f"📚 Source captured: {url}")

                if content:
                    # Truncate very long outputs for display
                    display_content = content[:500] + "..." if len(content) > 500 else content
                    print(f"📤 Output: {display_content}")
                print()  # Add spacing after tool output

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

