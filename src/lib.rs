//! PyO3 bindings: whole-URL crossing (P1). `parse` takes one `str`, returns one
//! 8-tuple of `Option<String>`/bool; `format_url` takes the 8 fields back.
//! The Python wrapper rebuilds the upstream `URL` NamedTuple, so the return
//! type is identical to stock mdurl. No per-token callbacks, no object graph
//! crosses the boundary (scalars only).

use mdurl_core::{parse as core_parse, ParsedUrl};
use pyo3::prelude::*;

/// One whole parsed URL: the 8 `URL` fields in order. Kept as a plain tuple
/// so the Python wrapper rebuilds the upstream `URL` NamedTuple unchanged.
type ParsedTuple = (
    Option<String>,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

#[allow(clippy::too_many_arguments)] // mirrors the 8 URL fields 1:1
fn format_fields(
    protocol: Option<String>,
    slashes: bool,
    auth: Option<String>,
    port: Option<String>,
    hostname: Option<String>,
    hash: Option<String>,
    search: Option<String>,
    pathname: Option<String>,
) -> String {
    let p = ParsedUrl {
        protocol,
        slashes,
        auth,
        port,
        hostname,
        hash,
        search,
        pathname,
    };
    mdurl_core::format_url(&p)
}

#[pyfunction]
#[pyo3(signature = (url, slashes_denote_host=false))]
fn parse(url: &str, slashes_denote_host: bool) -> ParsedTuple {
    let p = core_parse(url, slashes_denote_host);
    (
        p.protocol,
        p.slashes,
        p.auth,
        p.port,
        p.hostname,
        p.hash,
        p.search,
        p.pathname,
    )
}

#[pyfunction]
#[pyo3(signature = (protocol, slashes, auth, port, hostname, hash, search, pathname))]
#[allow(clippy::too_many_arguments)] // mirrors the 8 URL fields 1:1
fn format_url(
    protocol: Option<String>,
    slashes: bool,
    auth: Option<String>,
    port: Option<String>,
    hostname: Option<String>,
    hash: Option<String>,
    search: Option<String>,
    pathname: Option<String>,
) -> String {
    format_fields(protocol, slashes, auth, port, hostname, hash, search, pathname)
}

#[pymodule]
fn _core(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(format_url, m)?)?;
    Ok(())
}
