# Spec Delta

## Purpose

Serves the offered file list over HTTP so DBI's "Home server" network install can
browse and stream the same files the USB server offers, without a cable and
without a second copy of the library.

## ADDED Requirements

### Requirement: The bind address comes from the config file

The system SHALL read the HTTP server's bind address and port from an `[http]`
section of the user's configuration file, defaulting to every interface and port
`65080` when the section or a setting is absent. A malformed value SHALL produce
a warning and leave that setting at its default.

#### Scenario: Defaults apply without the section

- **WHEN** the configuration file has no `[http]` section
- **THEN** the server binds every interface on port `65080`

#### Scenario: Host and port are read

- **WHEN** the `[http]` section sets `host` and `port`
- **THEN** the server binds that address on that port

#### Scenario: A malformed port warns and defaults

- **WHEN** the `[http]` section sets `port` to a value that is not a port number
- **THEN** a warning is produced and the default port stays in effect

### Requirement: The HTTP server is started and stopped from the UI

The HTTP server SHALL run as a background task the user starts and stops, and
SHALL serve a snapshot of the file list taken when it starts. It SHALL run
independently of the USB server, and both servers MAY serve at the same time,
each over its own snapshot. A bind failure SHALL be reported once and end the
run, leaving the HTTP server startable again.

#### Scenario: Starting serves the current list

- **WHEN** the user starts the HTTP server
- **THEN** the files it lists and streams are those offered at the moment it started

#### Scenario: Edits after start are not served

- **WHEN** the file list changes after the HTTP server started
- **THEN** the server keeps listing and streaming the snapshot taken at start

#### Scenario: Both servers run at once

- **WHEN** the HTTP server is started while the USB server is active
- **THEN** both servers run and each serves its own snapshot

#### Scenario: A bind failure ends the run

- **WHEN** the HTTP server cannot bind its address, for example a taken port
- **THEN** the failure is logged once, the run ends, and the server can be started again

### Requirement: The root listing is an autoindex-style HTML page

A request for the server root SHALL return an HTML document in the Apache/nginx
autoindex style: one anchor per offered file, in list order, whose text is the
file's name and whose href percent-encodes that name for requesting it, shown
alongside the file's size. The listing SHALL name exactly the snapshot's files.

#### Scenario: Every offered file appears once

- **WHEN** the root listing is fetched with three files offered
- **THEN** the document holds three file anchors, each naming one file and its size

#### Scenario: A non-ASCII name round-trips

- **WHEN** the list holds a file whose name is not ASCII
- **THEN** the anchor's text shows the name as-is while its href percent-encodes it

#### Scenario: An empty list lists nothing

- **WHEN** the listing is fetched with no files offered
- **THEN** the document contains no file anchors

### Requirement: File downloads stream through the entry's source

A request for a listed file's href SHALL stream that entry's bytes through its
source — the local filesystem for a local entry, the entry's share for a share
entry. A name the snapshot does not hold SHALL be answered with a not-found
status. A download that fails mid-stream SHALL be logged naming the file.

#### Scenario: A known local file streams its bytes

- **WHEN** a listed local file is requested
- **THEN** the response body equals that file's contents

#### Scenario: A share file streams through the share

- **WHEN** a listed file added from a share is requested
- **THEN** the response body equals that file's contents on the share

#### Scenario: An unknown name is not found

- **WHEN** a name not in the snapshot is requested
- **THEN** the response carries a not-found status and no file is opened

### Requirement: Byte ranges are honored

The server SHALL answer a request whose `Range` header names a single `bytes`
range — bounded or open-ended — with exactly that slice of the file and a
`Content-Range` header, and SHALL answer a request without a range, or with a
range it does not understand, with the whole file. A range starting beyond the
file's end SHALL be refused as unsatisfiable. A `HEAD` request SHALL return the
same headers as its `GET` without a body.

#### Scenario: An open-ended range serves a tail slice

- **WHEN** a file is requested with `Range: bytes=100-`
- **THEN** the response is a partial-content one carrying the bytes from offset 100 to the end

#### Scenario: A bounded range serves an exact slice

- **WHEN** a file is requested with `Range: bytes=0-99`
- **THEN** the response carries exactly the first 100 bytes and a matching `Content-Range`

#### Scenario: No range serves the whole file

- **WHEN** a file is requested without a `Range` header
- **THEN** the response carries the whole file and its full length

#### Scenario: An unsatisfiable start is refused

- **WHEN** a file is requested with a range whose start is beyond the file's end
- **THEN** the response is a range-not-satisfiable one and no bytes are streamed

#### Scenario: HEAD returns headers only

- **WHEN** a listed file is requested with `HEAD`
- **THEN** the response carries the same headers as the `GET` and an empty body

### Requirement: Downloads report progress and completion like the USB server

While a file streams over HTTP, the server SHALL report that file and the bytes
transferred, and a file streamed to its end SHALL be reported complete once,
naming the file and its size. The server SHALL NOT log a line per request, per
listing fetch, nor per streamed chunk. Failures SHALL be logged.

#### Scenario: A download reports progress and completes once

- **WHEN** a file is downloaded to its end over HTTP
- **THEN** progress samples for it are reported and exactly one completion names it with its size

#### Scenario: Browsing the listing writes no log line

- **WHEN** the root listing is fetched
- **THEN** the log gains no line for that request

### Requirement: The server is unauthenticated and LAN-scoped

The server SHALL require no authentication, SHALL NOT accept credentials or
tokens in the configuration, and the documentation SHALL state that it serves
the offered files to every host that can reach its bind address.

#### Scenario: A download needs no credentials

- **WHEN** a listed file is requested without any credentials
- **THEN** the file is served

#### Scenario: The config offers no authentication setting

- **WHEN** the `[http]` section's settings are defined
- **THEN** they are the bind host and port, with no setting for authentication
