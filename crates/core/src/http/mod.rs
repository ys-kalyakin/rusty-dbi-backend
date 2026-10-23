//! Serving the offered files over HTTP, for DBI's "Home server" network
//! install: a listing an `ApacheHTTP` source can browse, and byte-range
//! downloads through the same sources the USB server streams from.

pub mod listing;
pub mod ranges;
pub mod server;
