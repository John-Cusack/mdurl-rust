"""Workload-corpus equivalence gate: every URL in the frozen WORKLOAD.md corpus
must parse and re-format identically under stock mdurl and mdurl_rust, with
the frozen outcome checksum (prevents silent drift)."""

import mdurl as stock
import mdurl_rust as port

FIELDS = ("protocol", "slashes", "auth", "port", "hostname", "hash", "search", "pathname")


def corpus():
    urls = []
    for i in range(2000):
        m = i % 4
        if m == 0:
            urls.append(
                f"https://user{i}:pass{i}@example{i % 100}.com:8080/p/{i}?a=1&b={i}#frag"
            )
        elif m == 1:
            urls.append(f"/relative/path/{i}?x={i}&y=z#s{i}")
        elif m == 2:
            urls.append(f"https://m\u00fcnchen-{i % 50}.de/\u00fcnicode/{i}")
        else:
            urls.append(f"mailto:user+tag-{i}@mail{i % 100}.example.com")
    return urls


def test_workload_corpus_agrees():
    n = 0
    for u in corpus():
        a = stock.parse(u)
        b = port.parse(u)
        assert tuple(getattr(a, f) for f in FIELDS) == tuple(
            getattr(b, f) for f in FIELDS
        )
        assert stock.format(a) == port.format(b)
        n += len(port.format(b))
    assert n == 85300
