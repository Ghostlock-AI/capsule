import os
import re
import warnings
from collections import deque
from typing import List, Tuple

from dotenv import load_dotenv
import dspy
try:
    # Prefer the new package to avoid rename warnings
    from ddgs import DDGS  # type: ignore
except Exception:
    # Suppress rename runtime warning if falling back
    warnings.filterwarnings(
        "ignore",
        message=r"This package \(`duckduckgo_search`\) has been renamed to `ddgs`!",
        category=RuntimeWarning,
    )
    from duckduckgo_search import DDGS  # type: ignore

# Load environment from .env if present
load_dotenv()

api_key = os.getenv("OPENAI_API_KEY")
lm = dspy.LM("openai/gpt-4o-mini", api_key=api_key)
dspy.configure(lm=lm)
# ----------------- Config -----------------
MAX_TURNS_MEMORY = 6  # keep the last N Q/A pairs

# Quiet known noisy warnings from dependencies
warnings.filterwarnings(
    "ignore",
    category=DeprecationWarning,
    module=r"litellm\._service_logger",
)
warnings.filterwarnings(
    "ignore",
    category=UserWarning,
    module=r"pydantic\.main",
)


# ----------------- Tools ------------------
def web_search(query: str, k: int = 6) -> List[Tuple[str, str]]:
    results = []
    with DDGS() as ddg:
        for r in ddg.text(query, max_results=k, safesearch="moderate"):
            title = r.get("title") or r.get("href") or r.get("url") or "result"
            snippet = r.get("body") or r.get("description") or ""
            url = r.get("href") or r.get("url") or ""
            label = f"{title} — {url}" if url else title
            results.append((label, snippet))
    return results


# ----------------- DSPy Signatures --------
class SearchAnswer(dspy.Signature):
    """Answer the user's question using web snippets and recent chat history.

    Keep a single textual output; compute cited sources in Python.
    """

    question: str = dspy.InputField()
    chat_history: str = dspy.InputField(
        desc="Recent Q/A pairs for context; be concise."
    )
    context_snippets: str = dspy.InputField(desc="Curated snippets from web search.")
    answer: str = dspy.OutputField(
        desc="Concise answer with inline citations like [1], [2]."
    )


# ----------------- Agent ------------------
class WebSearchAgent(dspy.Module):
    def __init__(self, k: int = 6):
        super().__init__()
        self.k = k
        self.predict = dspy.Predict(SearchAnswer)

    def _extract_cited_sources(self, text: str, all_sources: List[str]) -> List[str]:
        """Return sources in the order they are cited like [1], [2]."""
        if not text:
            return []
        seen = set()
        ordered = []
        for m in re.finditer(r"\[(\d+)\]", text):
            try:
                idx = int(m.group(1)) - 1
            except ValueError:
                continue
            if 0 <= idx < len(all_sources) and idx not in seen:
                seen.add(idx)
                ordered.append(all_sources[idx])
        return ordered

    def forward(self, question: str, chat_history_text: str = "") -> dspy.Prediction:
        hits = web_search(question, k=self.k)
        if not hits:
            return dspy.Prediction(
                answer="No credible results surfaced. Try a more specific question.",
                sources=[],
            )

        # Prepare numbered snippets for [n] citations
        lines, sources = [], []
        for i, (label, snippet) in enumerate(hits, start=1):
            sources.append(label)
            snippet = (snippet or "").strip()
            lines.append(f"[{i}] {label}\n{snippet}")
        context = "\n\n".join(lines[: self.k])

        pred = self.predict(
            question=question.strip(),
            chat_history=chat_history_text.strip(),
            context_snippets=context,
        )

        # Attach computed cited sources
        cited = self._extract_cited_sources(getattr(pred, "answer", ""), sources)
        if not cited:
            cited = sources[: min(3, len(sources))]
        setattr(pred, "sources", cited)
        return pred


# ----------------- REPL -------------------
def format_history(pairs: deque) -> str:
    """Turn [(user, assistant), ...] into a compact transcript."""
    parts = []
    for u, a in pairs:
        parts.append(f"User: {u}\nAssistant: {a}")
    return "\n---\n".join(parts)


def main():
    print("DSPy Web Agent REPL. Type 'exit' to quit.")
    agent = WebSearchAgent(k=6)
    memory: deque[tuple[str, str]] = deque(maxlen=MAX_TURNS_MEMORY)

    while True:
        try:
            q = input("\nYou: ").strip()
        except (EOFError, KeyboardInterrupt):
            print("\nBye.")
            break

        if q.lower() in {"exit", "quit", ":q"}:
            print("Bye.")
            break
        if not q:
            continue

        chat_hist_text = format_history(memory)
        pred = agent(q, chat_hist_text)

        # Print the answer
        ans = (pred.answer or "").strip()
        print("\nAssistant:")
        print(ans if ans else "(no answer)")

        # Print sources (if any)
        if getattr(pred, "sources", None):
            print("\nSources:")
            for i, s in enumerate(pred.sources, start=1):
                print(f"[{i}] {s}")

        # Update memory
        memory.append((q, ans))


if __name__ == "__main__":
    main()
