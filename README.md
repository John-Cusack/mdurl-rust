# mdurl-rust

Rust-accelerated Markdown URL utilities — a port of
[executablebooks/mdurl](https://github.com/executablebooks/mdurl) (MIT;
see [NOTICE](NOTICE)).

Ported against mdurl 0.1.2. `parse` + `format` run in a compiled kernel
with one crossing per URL (whole-unit crossing); `decode` / `encode` and
the `URL` type are re-exported from upstream `mdurl`. Importable
pure-Python fallback: if the extension is unavailable, `parse`/`format`
resolve to the upstream implementation with identical semantics.

## Layout

- `mdurl-core/` — parse/format kernel (pure Rust, no PyO3).
- `src/` — PyO3 bindings (`_core` extension module).
- `mdurl_rust/` — Python package (`parse`, `format`, fallback).
- `tests/` — upstream-suite gate (189 upstream tests adapted) +
  workload-corpus equivalence gate.

## API

```python
import mdurl_rust
u = mdurl_rust.parse("https://user:pass@example.com:8080/p?a=1#frag")
mdurl_rust.format(u)  # round-trips
```

`parse` accepts a `str` (or a `URL`, returned as-is) plus keyword-only
`slashes_denote_host`; returns the upstream `URL` NamedTuple. Byte-exact
fields and messages: the differential corpus (upstream fixtures +
2000-URL workload, both `slashes_denote_host` settings, 4176 pairs) plus a
20,000-input fuzz over controls/unicode/delimiters agree field-for-field,
including `format` round-trips.

## Bench

Frozen workload (`WORKLOAD.md` in the lane dir): 2000 mixed URLs
(auth+port+query+fragment, relative, unicode, mailto) x 300 reps =
600,000 parse+format ops. Ryzen 9 5950X, CPython 3.12.3, fresh-subprocess
`/usr/bin/time -v` walls, peak RSS both arms.

| track | stock (s) | port (s) | speedup | RSS stock -> port |
|---|---|---|---|---|
| representative (600k URLs) | 2.886 | 0.925 | **3.12x** | 11944 -> 12524 KiB |
| x1 (single parse, 200k) | 4.877 us | 1.321 us | **3.69x** | 11636 -> 12416 KiB |

Tier-1 pure-Python baseline (hoisted `#`-guard + joined `format`): 2.712 s
(1.03x over stock) — no shippable upstream win; the port is 3.06x over the
tier-1 baseline. Checksums agree on all tracks (85300 rep / 54 x1).
RSS is flat (~12 MB both arms).

## Build

Requires Rust (stable) and maturin>=1:

```sh
pip install "mdurl==0.1.2" maturin
maturin develop --release   # editable, or: maturin build --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python -m pytest tests/ -q
```
