"""Rust-accelerated port of `executablebooks/mdurl` (MIT).

`parse` / `format` run in the compiled `_core` kernel with one crossing per
URL (whole-unit crossing); `decode` / `encode` and the `URL` type are
re-exported from upstream `mdurl`. If the extension is unavailable, `parse`
and `format` fall back to the pure-Python implementation with identical
semantics.
"""

from mdurl import (  # noqa: F401
    DECODE_COMPONENT_CHARS,
    DECODE_DEFAULT_CHARS,
    ENCODE_COMPONENT_CHARS,
    ENCODE_DEFAULT_CHARS,
    URL,
    decode,
    encode,
)

try:
    from mdurl_rust._core import format_url as _format_core
    from mdurl_rust._core import parse as _parse_core

    _HAVE_CORE = True
except ImportError:  # pragma: no cover - fallback path
    _HAVE_CORE = False

if _HAVE_CORE:

    def parse(url, *, slashes_denote_host=False):  # noqa: A002
        if isinstance(url, URL):
            return url
        t = _parse_core(url, slashes_denote_host)
        return URL(*t)

    def format(url):  # noqa: A001
        return _format_core(
            url.protocol,
            url.slashes,
            url.auth,
            url.port,
            url.hostname,
            url.hash,
            url.search,
            url.pathname,
        )

else:  # pragma: no cover - fallback path
    from mdurl import format, parse  # noqa: F401
