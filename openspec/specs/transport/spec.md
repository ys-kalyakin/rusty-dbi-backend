# transport Specification

## Purpose

Defines the bulk-endpoint contract the server relies on and how the real USB
transport discovers and claims the Switch's DBI interface.

## Requirements

### Requirement: Bulk transport contract

A transport SHALL expose reading exactly a requested number of bytes and writing
all provided bytes; a read that yields fewer bytes than requested SHALL report a
short-read error.

#### Scenario: Short read is an error

- **WHEN** an endpoint returns fewer bytes than requested
- **THEN** the transport reports a short-read error naming the expected and actual counts

#### Scenario: Write sends the full buffer

- **WHEN** the server writes a buffer
- **THEN** the transport transmits all of its bytes

### Requirement: USB device discovery

The USB transport SHALL look for a device with vendor id `0x057E` and product id
`0x3000`, waiting and retrying about once per second while the device is absent,
then claim interface `0` and its IN endpoint `0x81` and OUT endpoint `0x01`.

#### Scenario: Device is absent

- **WHEN** no matching device is connected
- **THEN** the transport waits and retries until one appears

#### Scenario: Device is claimed

- **WHEN** a matching device is present
- **THEN** the transport claims interface 0 and both bulk endpoints

### Requirement: Packet-aligned IN transfers

A USB read SHALL request a buffer rounded up to a multiple of the IN endpoint's
maximum packet size, while returning only the requested bytes.

#### Scenario: Buffer is rounded up

- **WHEN** a read of a length that is not a multiple of the packet size is issued
- **THEN** the transfer requests the next packet-aligned size
