from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

from pydantic import BaseModel, Field


IGNORED_DIRS = {".git", ".venv", "__pycache__", "node_modules", "target", "dist", "build"}
SUPPORTED_SUFFIXES = {".py", ".rs", ".ts", ".tsx", ".js", ".jsx", ".go"}


class CodeSymbol(BaseModel):
    name: str
    kind: str
    path: str
    line: int


class DependencyEdge(BaseModel):
    source: str
    target: str
    kind: str
    line: int


class IndexedFile(BaseModel):
    path: str
    language: str
    sha256: str
    parser: str = "regex"
    ast_node_count: int = 0
    symbols: list[CodeSymbol] = Field(default_factory=list)
    imports: list[DependencyEdge] = Field(default_factory=list)
    calls: list[DependencyEdge] = Field(default_factory=list)


class RepositoryIndex(BaseModel):
    root: str
    files: list[IndexedFile] = Field(default_factory=list)
    dependency_graph: list[DependencyEdge] = Field(default_factory=list)
    architecture_summary: str = ""


def language_for(path: Path) -> str:
    return {
        ".py": "python",
        ".rs": "rust",
        ".ts": "typescript",
        ".tsx": "typescript",
        ".js": "javascript",
        ".jsx": "javascript",
        ".go": "go",
    }.get(path.suffix, "text")


class RepositoryIndexer:
    def __init__(self, cache_dir: str = ".octobot/index") -> None:
        self.cache_dir = Path(cache_dir)
        self.cache_dir.mkdir(parents=True, exist_ok=True)

    def index(self, root: str) -> RepositoryIndex:
        root_path = Path(root).resolve()
        cache_path = self.cache_dir / f"{hashlib.sha256(str(root_path).encode()).hexdigest()}.json"
        root_digest = self._root_digest(root_path)
        if cache_path.exists():
            cached = json.loads(cache_path.read_text(encoding="utf-8"))
            if cached.get("root_digest") == root_digest:
                return RepositoryIndex.model_validate(cached["index"])
        files: list[IndexedFile] = []
        for path in root_path.rglob("*"):
            if not path.is_file() or path.suffix not in SUPPORTED_SUFFIXES:
                continue
            if any(part in IGNORED_DIRS for part in path.parts):
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            rel_path = str(path.relative_to(root_path))
            language = language_for(path)
            ast_node_count = self._tree_sitter_node_count(text, language)
            parser = "tree-sitter" if ast_node_count else "regex"
            symbols = self._extract_symbols(rel_path, text)
            imports = self._extract_imports(rel_path, text, language)
            files.append(
                IndexedFile(
                    path=rel_path,
                    language=language,
                    sha256=hashlib.sha256(text.encode()).hexdigest(),
                    parser=parser,
                    ast_node_count=ast_node_count,
                    symbols=symbols,
                    imports=imports,
                    calls=self._extract_calls(rel_path, text, symbols),
                )
            )
        dependency_graph = [edge for file in files for edge in [*file.imports, *file.calls]]
        index = RepositoryIndex(
            root=str(root_path),
            files=files,
            dependency_graph=dependency_graph,
            architecture_summary=self._summarize(files, dependency_graph),
        )
        cache_path.write_text(
            json.dumps({"root_digest": root_digest, "index": index.model_dump()}, indent=2),
            encoding="utf-8",
        )
        return index

    def _extract_symbols(self, rel_path: str, text: str) -> list[CodeSymbol]:
        symbols: list[CodeSymbol] = []
        for idx, line in enumerate(text.splitlines(), start=1):
            stripped = line.strip()
            if stripped.startswith(("def ", "class ", "fn ", "struct ", "enum ", "trait ")):
                token = stripped.split()[1].split("(")[0].split("{")[0].rstrip(":")
                kind = stripped.split()[0]
                symbols.append(CodeSymbol(name=token, kind=kind, path=rel_path, line=idx))
            elif stripped.startswith("func "):
                token = stripped.split()[1].split("(")[0].rstrip(":")
                symbols.append(CodeSymbol(name=token, kind="func", path=rel_path, line=idx))
        return symbols

    def _extract_imports(self, rel_path: str, text: str, language: str) -> list[DependencyEdge]:
        edges: list[DependencyEdge] = []
        patterns = {
            "python": [
                re.compile(r"^\s*import\s+([A-Za-z0-9_\.]+)"),
                re.compile(r"^\s*from\s+([A-Za-z0-9_\.]+)\s+import\s+"),
            ],
            "rust": [re.compile(r"^\s*use\s+([A-Za-z0-9_:]+)")],
            "typescript": [re.compile(r"""from\s+["']([^"']+)["']""")],
            "javascript": [re.compile(r"""from\s+["']([^"']+)["']""")],
            "go": [re.compile(r"""^\s*"?([A-Za-z0-9_/\.-]+)"?\s*$""")],
        }
        active = patterns.get(language, [])
        in_go_import_block = False
        for idx, line in enumerate(text.splitlines(), start=1):
            if language == "go":
                if line.strip().startswith("import ("):
                    in_go_import_block = True
                    continue
                if in_go_import_block and line.strip() == ")":
                    in_go_import_block = False
                    continue
                if not in_go_import_block and not line.strip().startswith("import "):
                    continue
                line = line.replace("import ", "").strip()
            for pattern in active:
                match = pattern.search(line)
                if match:
                    edges.append(
                        DependencyEdge(
                            source=rel_path,
                            target=match.group(1),
                            kind="import",
                            line=idx,
                        )
                    )
                    break
        return edges

    def _extract_calls(
        self, rel_path: str, text: str, local_symbols: list[CodeSymbol]
    ) -> list[DependencyEdge]:
        symbol_names = {symbol.name for symbol in local_symbols}
        edges: list[DependencyEdge] = []
        for idx, line in enumerate(text.splitlines(), start=1):
            for name in symbol_names:
                if re.search(rf"\b{re.escape(name)}\s*\(", line) and not line.strip().startswith(
                    ("def ", "fn ", "func ")
                ):
                    edges.append(DependencyEdge(source=rel_path, target=name, kind="call", line=idx))
        return edges

    def _summarize(self, files: list[IndexedFile], edges: list[DependencyEdge]) -> str:
        by_lang: dict[str, int] = {}
        for file in files:
            by_lang[file.language] = by_lang.get(file.language, 0) + 1
        lang_summary = ", ".join(f"{lang}: {count}" for lang, count in sorted(by_lang.items()))
        symbol_count = sum(len(file.symbols) for file in files)
        return (
            f"{len(files)} source files indexed ({lang_summary}); "
            f"{symbol_count} symbols; {len(edges)} dependency/call edges; "
            f"{sum(1 for file in files if file.parser == 'tree-sitter')} tree-sitter parsed files."
        )

    def _root_digest(self, root_path: Path) -> str:
        digest = hashlib.sha256()
        for path in sorted(root_path.rglob("*")):
            if not path.is_file() or path.suffix not in SUPPORTED_SUFFIXES:
                continue
            if any(part in IGNORED_DIRS for part in path.parts):
                continue
            stat = path.stat()
            digest.update(str(path.relative_to(root_path)).encode())
            digest.update(str(stat.st_mtime_ns).encode())
            digest.update(str(stat.st_size).encode())
        return digest.hexdigest()

    def _tree_sitter_node_count(self, text: str, language: str) -> int:
        parser = self._tree_sitter_parser(language)
        if parser is None:
            return 0
        try:
            tree = parser.parse(text.encode("utf-8"))
        except Exception:
            return 0
        return _count_tree_sitter_nodes(tree.root_node)

    def _tree_sitter_parser(self, language: str):
        try:
            from tree_sitter import Language, Parser
        except Exception:
            return None
        grammar_modules = {
            "python": ("tree_sitter_python", "language"),
            "rust": ("tree_sitter_rust", "language"),
            "typescript": ("tree_sitter_typescript", "language_typescript"),
            "javascript": ("tree_sitter_javascript", "language"),
            "go": ("tree_sitter_go", "language"),
        }
        module_name, function_name = grammar_modules.get(language, ("", ""))
        if not module_name:
            return None
        try:
            module = __import__(module_name)
            language_capsule = getattr(module, function_name)()
            parser = Parser()
            parsed_language = Language(language_capsule)
            if hasattr(parser, "set_language"):
                parser.set_language(parsed_language)
            else:
                parser.language = parsed_language
            return parser
        except Exception:
            return None


def _count_tree_sitter_nodes(node) -> int:
    return 1 + sum(_count_tree_sitter_nodes(child) for child in node.children)
