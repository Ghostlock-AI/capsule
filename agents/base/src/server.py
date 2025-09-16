# ── server.py ────────────────────────────────────────────────────────────────
import os
from typing import Annotated, AsyncIterator

from dotenv import load_dotenv
from fastapi import FastAPI, HTTPException
from fastapi.responses import StreamingResponse
from langchain.chat_models import init_chat_model
from langgraph.graph import START, StateGraph
from langgraph.graph.message import add_messages
from typing_extensions import TypedDict

load_dotenv()
api_key = os.getenv("OPENAI_API_KEY")
if not api_key:
    raise RuntimeError("OPENAI_API_KEY not set")


# ---------- LangGraph plumbing (unchanged) -----------------------------------
class State(TypedDict):
    messages: Annotated[list, add_messages]  # conversation history


def build_graph() -> StateGraph:
    llm = init_chat_model(
        "openai:gpt-4o",
        openai_api_key=api_key,
        streaming=True,
    )
    gb = StateGraph(State)

    def chatbot(state: State) -> dict:
        return {"messages": [llm.invoke(state["messages"])]}

    gb.add_node("chatbot", chatbot)
    gb.add_edge(START, "chatbot")
    return gb.compile()


GRAPH = build_graph()

# ---------- FastAPI -----------------------------------------------------------
app = FastAPI()


async def _token_stream(user_message: str) -> AsyncIterator[str]:
    try:
        for event in GRAPH.stream(
            {"messages": [{"role": "user", "content": user_message}]}
        ):
            for value in event.values():
                token = value["messages"][-1].content
                # Yield *only* incremental delta to avoid duplication
                yield token
    except Exception as e:
        # surfaces nicely in client
        yield f"\n[ERROR] {e}\n"


@app.post("/chat")
async def chat(payload: dict):
    msg = payload.get("message", "")
    if not msg:
        raise HTTPException(422, "message field required")

    return StreamingResponse(
        _token_stream(msg), media_type="text/plain"
    )  # simplest: chunked text


if __name__ == "__main__":
    import uvicorn

    uvicorn.run("server:app", host="127.0.0.1", port=8000, reload=True)
