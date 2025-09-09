from __future__ import annotations

import os
from typing import List, Tuple, Any, Dict

import requests


def _coerce_item(item: Dict[str, Any]) -> Tuple[str, str]:
    """Best-effort coercion of a generic result item into (label, snippet).

    Tries common keys like title/url/text/body/description. Falls back safely.
    """
    title = (
        item.get("title")
        or item.get("name")
        or item.get("label")
        or item.get("url")
        or item.get("href")
        or "result"
    )
    url = item.get("url") or item.get("href") or ""
    snippet = (
        item.get("snippet")
        or item.get("text")
        or item.get("body")
        or item.get("description")
        or ""
    )
    label = f"{title} — {url}" if url else str(title)
    return label, str(snippet)


def run_base_tool(query: str, k: int = 6) -> List[Tuple[str, str]]:
    """Call a configurable Base Tool service and return list of (label, snippet).

    Configuration via env vars:
    - BASE_TOOL_URL: Base endpoint (e.g., http://localhost:8000/search)
    - BASE_TOOL_METHOD: HTTP method, GET or POST (default: GET)
    - BASE_TOOL_TOKEN: Optional bearer/API token for Authorization
    - BASE_TOOL_TIMEOUT: Request timeout seconds (default: 10)

    Expected response formats (best-effort parsing):
    - { "results": [ { "title": ..., "url": ..., "snippet": ... }, ... ] }
    - [ { "title": ..., "url": ..., "text": ... }, ... ]
    - { "data": { "items": [ ... ] } }
    """
    base_url = os.getenv("BASE_TOOL_URL")
    if not base_url:
        return []

    method = (os.getenv("BASE_TOOL_METHOD") or "GET").upper()
    token = os.getenv("BASE_TOOL_TOKEN")
    timeout_s = float(os.getenv("BASE_TOOL_TIMEOUT") or 10)

    headers = {"Accept": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"

    params = {"q": query, "k": k}
    payload: Dict[str, Any] = {"q": query, "k": k}

    try:
        if method == "POST":
            resp = requests.post(base_url, json=payload, headers=headers, timeout=timeout_s)
        else:
            resp = requests.get(base_url, params=params, headers=headers, timeout=timeout_s)
        resp.raise_for_status()
        data = resp.json()
    except Exception:
        return []

    # Normalize a few common shapes
    items: List[Dict[str, Any]] = []
    if isinstance(data, list):
        items = [x for x in data if isinstance(x, dict)]
    elif isinstance(data, dict):
        if isinstance(data.get("results"), list):
            items = [x for x in data.get("results", []) if isinstance(x, dict)]
        elif isinstance(data.get("data"), dict) and isinstance(data["data"].get("items"), list):
            items = [x for x in data["data"].get("items", []) if isinstance(x, dict)]
        else:
            # Last resort: collect any list-like value
            for v in data.values():
                if isinstance(v, list):
                    items = [x for x in v if isinstance(x, dict)]
                    if items:
                        break

    results: List[Tuple[str, str]] = []
    for item in items[:k]:
        label, snippet = _coerce_item(item)
        # Prefix label with source tag so citations show provenance
        results.append((f"base: {label}", snippet))
    return results

