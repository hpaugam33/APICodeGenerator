"""Name normalization helpers.

EA class/attribute names can contain spaces, accents or mixed casing
(e.g. "Détails de la commande", "orderId"). All generated identifiers go
through these helpers so output is deterministic and language-safe.
"""

import re
import unicodedata


def normalize(text) -> str:
    """Strip accents; non-alphanumeric characters become word separators."""
    if text is None:
        return ""
    text = unicodedata.normalize("NFKD", str(text))
    text = "".join(c for c in text if not unicodedata.combining(c))
    parts = re.split(r"[^0-9A-Za-z]+", text)
    return " ".join(p for p in parts if p)


def words(text) -> list:
    """Split an identifier into words, handling camelCase/PascalCase/ALL_CAPS too."""
    n = normalize(text)
    if not n:
        return []
    n = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", " ", n)
    n = re.sub(r"(?<=[A-Z])(?=[A-Z][a-z])", " ", n)
    out = []
    for p in re.split(r"\s+", n):
        if not p:
            continue
        if len(p) > 1 and p.isupper():
            p = p.lower()
        out.append(p)
    return out


def pascal(text) -> str:
    w = words(text)
    return "".join(p[:1].upper() + p[1:] for p in w) or "X"


def camel(text) -> str:
    p = pascal(text)
    return p[:1].lower() + p[1:]


def snake(text) -> str:
    w = words(text)
    return "_".join(p.lower() for p in w) or "x"


def kebab(text) -> str:
    w = words(text)
    return "-".join(p.lower() for p in w) or "x"


def screaming(text) -> str:
    w = words(text)
    return "_".join(p.upper() for p in w) or "X"
