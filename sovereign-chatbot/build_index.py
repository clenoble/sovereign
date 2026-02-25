#!/usr/bin/env python3
"""Build the RAG embedding index for the Sovereign GE chatbot.

Indexes three source types:
  A) Website documentation HTML pages (from gh-pages branch)
  B) Rust codebase doc comments + public signatures (from main branch)
  C) Architecture & design markdown docs (from main branch)

Usage:
  # From the project root on main branch:
  python sovereign-chatbot/build_index.py \
      --html-dir /tmp/sovereign-gh-pages \
      --code-dir . \
      --output-dir sovereign-chatbot

  # If you don't have a gh-pages checkout, fetch from live site:
  python sovereign-chatbot/build_index.py \
      --fetch-html \
      --code-dir . \
      --output-dir sovereign-chatbot

Requires MISTRAL_API_KEY environment variable.
"""

import argparse
import json
import os
import re
import sys
from pathlib import Path

import numpy as np

# Optional: bs4 for HTML parsing
try:
    from bs4 import BeautifulSoup

    HAS_BS4 = True
except ImportError:
    HAS_BS4 = False

# Optional: requests for fetching from live site
try:
    import requests

    HAS_REQUESTS = True
except ImportError:
    HAS_REQUESTS = False


SITE_BASE_URL = "https://clenoble.github.io/sovereign"

DETAIL_PAGES = [
    "detail-action-gravity.html",
    "detail-ai-orchestrator.html",
    "detail-communications.html",
    "detail-content-skills.html",
    "detail-encryption.html",
    "detail-ethics.html",
    "detail-prompt-injection.html",
    "detail-social-recovery.html",
    "detail-spatial-canvas.html",
]

# Semantic domains: map crate/module paths to domain names
CODE_DOMAINS = {
    "sovereign-ai/src/orchestrator": "AI Orchestrator",
    "sovereign-ai/src/tools": "AI Tools",
    "sovereign-ai/src/intent": "Intent Classification",
    "sovereign-ai/src/llm": "LLM Backend & Prompts",
    "sovereign-ai/src/action_gate": "Action Gate (Gravity)",
    "sovereign-ai/src/trust": "Trust Calibration",
    "sovereign-ai/src/injection": "Prompt Injection Defense",
    "sovereign-ai/src/session_log": "Session Logging",
    "sovereign-ai/src/voice": "Voice Pipeline",
    "sovereign-ai/src/autocommit": "Auto-Commit",
    "sovereign-db/src/traits": "Database Trait (GraphDB)",
    "sovereign-db/src/schema": "Database Schema",
    "sovereign-db/src/surreal": "SurrealDB Implementation",
    "sovereign-crypto/src": "Encryption & Key Management",
    "sovereign-canvas/src": "Spatial Canvas",
    "sovereign-ui/src": "UI Components",
    "sovereign-comms/src": "Communications",
    "sovereign-p2p/src": "P2P Sync",
    "sovereign-skills/src": "Skills System",
    "sovereign-core/src": "Core Types & Config",
    "sovereign-app/src": "Application Entry Point",
}

# Design docs to include
DESIGN_DOCS = [
    ("CLAUDE.md", "Project Architecture"),
    ("doc/spec/sovereign_os_ux_principles.md", "UX Principles"),
    ("doc/design/design_decisions.md", "Design Decisions"),
    ("doc/spec/sovereign_os_specification.md", "GE Specification"),
    ("doc/legal/sovereign_os_ethics.md", "Ethics Charter"),
]


def chunk_text(text: str, max_chars: int = 1500, overlap: int = 100) -> list[str]:
    """Split text into chunks respecting paragraph boundaries."""
    paragraphs = re.split(r"\n{2,}", text.strip())
    result = []
    current = ""

    for para in paragraphs:
        para = para.strip()
        if not para:
            continue
        if len(current) + len(para) + 2 > max_chars and current:
            result.append(current.strip())
            # Keep overlap from end of previous chunk
            current = current[-overlap:] + "\n\n" + para if overlap else para
        else:
            current = current + "\n\n" + para if current else para

    if current.strip():
        result.append(current.strip())
    return result


# --- Source A: HTML documentation ---


def extract_html_sections(html: str, page_name: str) -> list[dict]:
    """Extract text sections from an HTML detail page."""
    if not HAS_BS4:
        print("WARNING: beautifulsoup4 not installed, skipping HTML parsing")
        return []

    soup = BeautifulSoup(html, "html.parser")

    # Remove non-content elements
    for tag in soup(["script", "style", "nav", "footer", "header", "svg"]):
        tag.decompose()

    chunks = []
    # Split by h2/h3 headings
    sections = []
    current_heading = page_name
    current_text = ""

    for element in soup.find_all(["h1", "h2", "h3", "p", "li", "pre", "code", "td"]):
        if element.name in ("h1", "h2", "h3"):
            if current_text.strip():
                sections.append((current_heading, current_text.strip()))
            current_heading = element.get_text(strip=True)
            current_text = ""
        else:
            text = element.get_text(strip=True)
            if text and len(text) > 20:  # skip tiny fragments
                current_text += text + "\n\n"

    if current_text.strip():
        sections.append((current_heading, current_text.strip()))

    # Chunk each section
    for heading, text in sections:
        for chunk_text_piece in chunk_text(text, max_chars=1500):
            if len(chunk_text_piece) > 50:  # skip tiny chunks
                chunks.append(
                    {
                        "text": chunk_text_piece,
                        "source": page_name,
                        "section": heading,
                        "type": "docs",
                    }
                )

    return chunks


def load_html_from_dir(html_dir: str) -> list[dict]:
    """Load and chunk HTML detail pages from a local directory."""
    all_chunks = []
    html_path = Path(html_dir)

    for page in DETAIL_PAGES:
        filepath = html_path / page
        if filepath.exists():
            html = filepath.read_text(encoding="utf-8")
            name = page.replace("detail-", "").replace(".html", "").replace("-", " ").title()
            page_chunks = extract_html_sections(html, name)
            all_chunks.extend(page_chunks)
            print(f"  {page}: {len(page_chunks)} chunks")
        else:
            print(f"  {page}: NOT FOUND at {filepath}")

    # Also try to extract from index.html (knowledge base entries)
    index_path = html_path / "index.html"
    if index_path.exists():
        html = index_path.read_text(encoding="utf-8")
        # Extract SOVEREIGN_KNOWLEDGE entries via regex
        match = re.search(
            r"const SOVEREIGN_KNOWLEDGE\s*=\s*\{(.+?)\};",
            html,
            re.DOTALL,
        )
        if match:
            # Parse the JS object entries (template literals)
            entries = re.findall(r"(\w+):\s*`([^`]+)`", match.group(1))
            for topic, text in entries:
                clean = re.sub(r"\s+", " ", text.strip())
                if len(clean) > 50:
                    all_chunks.append(
                        {
                            "text": clean,
                            "source": "index.html",
                            "section": topic.title(),
                            "type": "docs",
                        }
                    )
            print(f"  index.html knowledge base: {len(entries)} entries")

    return all_chunks


def fetch_html_from_site() -> list[dict]:
    """Fetch HTML pages from the live GitHub Pages site."""
    if not HAS_REQUESTS:
        print("ERROR: 'requests' package needed for --fetch-html. pip install requests")
        return []

    all_chunks = []
    for page in DETAIL_PAGES:
        url = f"{SITE_BASE_URL}/{page}"
        print(f"  Fetching {url}...")
        try:
            resp = requests.get(url, timeout=30)
            resp.raise_for_status()
            name = page.replace("detail-", "").replace(".html", "").replace("-", " ").title()
            page_chunks = extract_html_sections(resp.text, name)
            all_chunks.extend(page_chunks)
            print(f"    -> {len(page_chunks)} chunks")
        except Exception as e:
            print(f"    -> FAILED: {e}")

    # Fetch index.html for knowledge base
    try:
        resp = requests.get(f"{SITE_BASE_URL}/index.html", timeout=30)
        resp.raise_for_status()
        match = re.search(
            r"const SOVEREIGN_KNOWLEDGE\s*=\s*\{(.+?)\};",
            resp.text,
            re.DOTALL,
        )
        if match:
            entries = re.findall(r"(\w+):\s*`([^`]+)`", match.group(1))
            for topic, text in entries:
                clean = re.sub(r"\s+", " ", text.strip())
                if len(clean) > 50:
                    all_chunks.append(
                        {
                            "text": clean,
                            "source": "index.html",
                            "section": topic.title(),
                            "type": "docs",
                        }
                    )
            print(f"  index.html knowledge base: {len(entries)} entries")
    except Exception as e:
        print(f"  index.html: FAILED ({e})")

    return all_chunks


# --- Source B: Rust codebase ---


def classify_domain(filepath: str) -> str:
    """Map a file path to a semantic domain name."""
    # Normalize path separators
    fp = filepath.replace("\\", "/")
    for pattern, domain in CODE_DOMAINS.items():
        if pattern in fp:
            return domain
    return "Other"


def extract_rust_docs(filepath: Path) -> list[dict]:
    """Extract doc comments and public signatures from a Rust file."""
    try:
        content = filepath.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        return []

    lines = content.split("\n")
    items = []
    current_doc = []
    in_test_module = False
    rel_path = str(filepath).replace("\\", "/")

    for line in lines:
        stripped = line.strip()

        # Skip test modules
        if stripped.startswith("#[cfg(test)]"):
            in_test_module = True
            continue
        if in_test_module:
            if stripped.startswith("mod "):
                continue
            # Simple heuristic: tests end at the module's closing brace
            # but for extraction purposes, just skip everything after #[cfg(test)]
            continue

        # Collect module-level doc comments
        if stripped.startswith("//!"):
            doc_text = stripped[3:].strip()
            if doc_text:
                current_doc.append(doc_text)
            continue

        # Collect item doc comments
        if stripped.startswith("///"):
            doc_text = stripped[3:].strip()
            if doc_text:
                current_doc.append(doc_text)
            continue

        # Check for public item signatures
        if stripped.startswith("pub ") and not stripped.startswith("pub(crate)"):
            signature = stripped
            # Trim to just the signature (up to opening brace or semicolon)
            for end_char in ["{", ";", "where"]:
                idx = signature.find(end_char)
                if idx > 0:
                    signature = signature[:idx].strip()
                    break

            doc = "\n".join(current_doc) if current_doc else ""
            if doc or len(signature) > 10:
                domain = classify_domain(rel_path)
                items.append(
                    {
                        "doc": doc,
                        "signature": signature,
                        "domain": domain,
                        "file": rel_path,
                    }
                )
            current_doc = []
        elif not stripped.startswith("//"):
            # Non-comment, non-pub line: reset doc accumulator
            if current_doc and not stripped.startswith("#["):
                # Module-level docs at the top of the file
                if all(not item.get("signature") for item in items) and items == []:
                    domain = classify_domain(rel_path)
                    items.append(
                        {
                            "doc": "\n".join(current_doc),
                            "signature": "",
                            "domain": domain,
                            "file": rel_path,
                        }
                    )
                current_doc = []

    # Flush remaining module-level docs
    if current_doc:
        domain = classify_domain(rel_path)
        items.append(
            {
                "doc": "\n".join(current_doc),
                "signature": "",
                "domain": domain,
                "file": rel_path,
            }
        )

    return items


def load_rust_code(code_dir: str) -> list[dict]:
    """Walk Rust crates and extract doc comments + signatures, grouped by domain."""
    code_path = Path(code_dir) / "crates"
    if not code_path.exists():
        print(f"  Crates directory not found: {code_path}")
        return []

    # Collect all items grouped by domain
    domain_items: dict[str, list[dict]] = {}

    for rs_file in sorted(code_path.rglob("*.rs")):
        items = extract_rust_docs(rs_file)
        for item in items:
            domain = item["domain"]
            domain_items.setdefault(domain, []).append(item)

    # Also extract crate descriptions from Cargo.toml
    for cargo_path in sorted(code_path.glob("*/Cargo.toml")):
        content = cargo_path.read_text(encoding="utf-8")
        name_match = re.search(r'^name\s*=\s*"([^"]+)"', content, re.MULTILINE)
        desc_match = re.search(r'^description\s*=\s*"([^"]+)"', content, re.MULTILINE)
        if name_match and desc_match:
            crate_name = name_match.group(1)
            description = desc_match.group(1)
            domain = classify_domain(f"{crate_name}/src")
            domain_items.setdefault(domain, []).insert(
                0,
                {
                    "doc": f"Crate: {crate_name}\n{description}",
                    "signature": "",
                    "domain": domain,
                    "file": str(cargo_path).replace("\\", "/"),
                },
            )

    # Build chunks per domain
    all_chunks = []
    for domain, items in sorted(domain_items.items()):
        # Combine items into domain text
        parts = []
        for item in items:
            text = ""
            if item["doc"]:
                text += item["doc"]
            if item["signature"]:
                text += "\n" + item["signature"] if text else item["signature"]
            if text.strip():
                parts.append(text.strip())

        domain_text = "\n\n".join(parts)
        if not domain_text.strip():
            continue

        # Chunk the domain text
        for chunk_piece in chunk_text(domain_text, max_chars=2000):
            if len(chunk_piece) > 50:
                all_chunks.append(
                    {
                        "text": chunk_piece,
                        "source": items[0]["file"].split("crates/")[-1]
                        if "crates/" in items[0]["file"]
                        else items[0]["file"],
                        "section": domain,
                        "type": "code",
                    }
                )

        print(f"  {domain}: {len(items)} items -> {len([c for c in all_chunks if c['section'] == domain])} chunks")

    return all_chunks


# --- Source C: Design & architecture docs ---


def load_design_docs(code_dir: str) -> list[dict]:
    """Load architecture and design markdown documents."""
    all_chunks = []
    code_path = Path(code_dir)

    for rel_path, doc_name in DESIGN_DOCS:
        filepath = code_path / rel_path
        if not filepath.exists():
            print(f"  {rel_path}: NOT FOUND")
            continue

        content = filepath.read_text(encoding="utf-8")

        # Split by markdown headings
        sections = re.split(r"\n(#{1,3}\s+.+)\n", content)
        current_heading = doc_name
        current_text = ""

        i = 0
        while i < len(sections):
            section = sections[i].strip()
            if re.match(r"^#{1,3}\s+", section):
                # This is a heading
                if current_text.strip():
                    for chunk_piece in chunk_text(current_text, max_chars=1500):
                        if len(chunk_piece) > 50:
                            all_chunks.append(
                                {
                                    "text": chunk_piece,
                                    "source": rel_path,
                                    "section": current_heading,
                                    "type": "design",
                                }
                            )
                current_heading = section.lstrip("#").strip()
                current_text = ""
            else:
                current_text += section + "\n\n"
            i += 1

        # Flush last section
        if current_text.strip():
            for chunk_piece in chunk_text(current_text, max_chars=1500):
                if len(chunk_piece) > 50:
                    all_chunks.append(
                        {
                            "text": chunk_piece,
                            "source": rel_path,
                            "section": current_heading,
                            "type": "design",
                        }
                    )

        section_count = len(set(c["section"] for c in all_chunks if c["source"] == rel_path))
        print(f"  {rel_path}: {section_count} sections")

    return all_chunks


# --- Embedding ---


def embed_chunks(chunks: list[dict], api_key: str) -> np.ndarray:
    """Embed all chunks using Mistral embed API (rate-limited for free tier)."""
    import time
    from mistralai import Mistral

    client = Mistral(api_key=api_key)

    BATCH_SIZE = 25
    RATE_LIMIT_PAUSE = 35  # seconds between batches (free tier: 2 req/min)
    all_embeddings = []
    total_batches = (len(chunks) + BATCH_SIZE - 1) // BATCH_SIZE

    for i in range(0, len(chunks), BATCH_SIZE):
        batch = chunks[i : i + BATCH_SIZE]
        texts = [c["text"] for c in batch]
        batch_num = i // BATCH_SIZE + 1

        if batch_num > 1:
            print(f"  Rate limit pause ({RATE_LIMIT_PAUSE}s)...")
            time.sleep(RATE_LIMIT_PAUSE)

        print(f"  Embedding batch {batch_num}/{total_batches} ({len(texts)} texts)...")
        response = client.embeddings.create(model="mistral-embed", inputs=texts)

        for item in response.data:
            all_embeddings.append(item.embedding)

    return np.array(all_embeddings, dtype=np.float32)


# --- Main ---


def main():
    parser = argparse.ArgumentParser(description="Build RAG index for Sovereign GE chatbot")
    parser.add_argument("--html-dir", help="Local directory with gh-pages HTML files")
    parser.add_argument("--fetch-html", action="store_true", help="Fetch HTML from live GitHub Pages site")
    parser.add_argument("--code-dir", default=".", help="Root of the project (main branch checkout)")
    parser.add_argument("--output-dir", default=".", help="Where to write chunks.json and embeddings.npy")
    args = parser.parse_args()

    api_key = os.getenv("MISTRAL_API_KEY")
    if not api_key:
        print("ERROR: MISTRAL_API_KEY environment variable is required.")
        sys.exit(1)

    all_chunks = []

    # Source A: HTML documentation
    print("\n=== Source A: Website documentation ===")
    if args.html_dir:
        html_chunks = load_html_from_dir(args.html_dir)
    elif args.fetch_html:
        html_chunks = fetch_html_from_site()
    else:
        print("  Skipped (provide --html-dir or --fetch-html)")
        html_chunks = []
    all_chunks.extend(html_chunks)
    print(f"  Total docs chunks: {len(html_chunks)}")

    # Source B: Rust codebase
    print("\n=== Source B: Rust codebase ===")
    code_chunks = load_rust_code(args.code_dir)
    all_chunks.extend(code_chunks)
    print(f"  Total code chunks: {len(code_chunks)}")

    # Source C: Design docs
    print("\n=== Source C: Architecture & design docs ===")
    design_chunks = load_design_docs(args.code_dir)
    all_chunks.extend(design_chunks)
    print(f"  Total design chunks: {len(design_chunks)}")

    print(f"\n=== Total: {len(all_chunks)} chunks ===")

    if not all_chunks:
        print("No chunks to embed. Check your input paths.")
        sys.exit(1)

    # Embed
    print("\n=== Embedding with Mistral ===")
    embeddings = embed_chunks(all_chunks, api_key)
    print(f"  Embedding matrix shape: {embeddings.shape}")

    # Save
    output_path = Path(args.output_dir)
    output_path.mkdir(parents=True, exist_ok=True)

    chunks_file = output_path / "chunks.json"
    embeddings_file = output_path / "embeddings.npy"

    with open(chunks_file, "w", encoding="utf-8") as f:
        json.dump(all_chunks, f, indent=2, ensure_ascii=False)
    np.save(str(embeddings_file), embeddings)

    print(f"\n=== Saved ===")
    print(f"  {chunks_file} ({chunks_file.stat().st_size / 1024:.1f} KB)")
    print(f"  {embeddings_file} ({embeddings_file.stat().st_size / 1024:.1f} KB)")


if __name__ == "__main__":
    main()
