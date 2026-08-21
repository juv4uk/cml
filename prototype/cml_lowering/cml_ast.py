"""S-Expression Tokenizer, AST Node Hierarchy, and Printer for CML Phonetic Lowering."""

import re
from dataclasses import dataclass
from typing import Any, List, Tuple, Union

@dataclass(frozen=True)
class IntLit:
    val: int

@dataclass(frozen=True)
class StrLit:
    val: str

@dataclass(frozen=True)
class SymLit:
    name: str

@dataclass(frozen=True)
class QuoteNode:
    inner: Any

@dataclass(frozen=True)
class ListNode:
    items: List[Any]

Expr = Union[IntLit, StrLit, SymLit, QuoteNode, ListNode]

def tokenize(source: str) -> List[str]:
    # Match strings, parentheses, single-quote, and symbols/numbers
    token_pattern = re.compile(r"""
        (?P<SPACE>\s+)
      | (?P<COMMENT>;[^\n]*)
      | (?P<STR>"[^"\\]*(?:\\.[^"\\]*)*")
      | (?P<LPAREN>\()
      | (?P<RPAREN>\))
      | (?P<QUOTE>')
      | (?P<ATOM>[^\s();]+)
    """, re.VERBOSE)

    tokens = []
    for match in token_pattern.finditer(source):
        kind = match.lastgroup
        text = match.group()
        if kind in ("SPACE", "COMMENT"):
            continue
        tokens.append(text)
    return tokens

def parse_tokens(tokens: List[str]) -> Tuple[List[Any], int]:
    result = []
    i = 0
    while i < len(tokens):
        tok = tokens[i]
        if tok == '(':
            sub_items, consumed = parse_list(tokens[i+1:])
            result.append(ListNode(sub_items))
            i += 1 + consumed
        elif tok == ')':
            raise SyntaxError("Unexpected closing parenthesis")
        elif tok == "'":
            # quote shorthand
            if i + 1 >= len(tokens):
                raise SyntaxError("Trailing quote with no operand")
            sub_res, consumed = parse_tokens(tokens[i+1:i+2])
            result.append(QuoteNode(sub_res[0]))
            i += 1 + consumed
        else:
            result.append(_parse_atom(tok))
            i += 1
    return result

def parse_list(tokens: List[str]) -> Tuple[List[Any], int]:
    items = []
    i = 0
    while i < len(tokens):
        tok = tokens[i]
        if tok == ')':
            return items, i + 1
        if tok == '(':
            sub_items, consumed = parse_list(tokens[i+1:])
            items.append(ListNode(sub_items))
            i += 1 + consumed
        elif tok == "'":
            if i + 1 >= len(tokens):
                raise SyntaxError("Trailing quote with no operand")
            sub_res, consumed = parse_tokens(tokens[i+1:i+2])
            items.append(QuoteNode(sub_res[0]))
            i += 1 + consumed
        else:
            items.append(_parse_atom(tok))
            i += 1
    raise SyntaxError("Unclosed parenthesis")

def _parse_atom(tok: str) -> Any:
    try:
        if tok.startswith("0x") or tok.startswith("0X"):
            return IntLit(int(tok, 16))
        return IntLit(int(tok))
    except ValueError:
        pass
    if tok.startswith('"') and tok.endswith('"'):
        return StrLit(tok[1:-1])
    return SymLit(tok)

def parse(source: str) -> List[Any]:
    tokens = tokenize(source)
    return parse_tokens(tokens)

def to_s_expr(node: Any) -> str:
    if isinstance(node, IntLit):
        return f"0x{node.val:016X}" if node.val > 0xFFFF else str(node.val)
    if isinstance(node, StrLit):
        return f'"{node.val}"'
    if isinstance(node, SymLit):
        return node.name
    if isinstance(node, QuoteNode):
        return f"(quote {to_s_expr(node.inner)})"
    if isinstance(node, ListNode):
        return "(" + " ".join(to_s_expr(x) for x in node.items) + ")"
    return str(node)
