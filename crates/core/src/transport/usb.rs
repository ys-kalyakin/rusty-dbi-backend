//! Real USB transport backed by `nusb`.

use std::time::Duration;

use async_trait::async_trait;
use nusb::transfer::{Buffer, Bulk, In, Out};
use nusb::{list_devices, Interface};

use super::{round_up, Transport, TransportError};

pub const VENDOR_ID: u16 = 0x057E;
pub const PRODUCT_ID: u16 = 0x3000;
pub const INTERFACE_NUMBER: u8 = 0;
pub const ENDPOINT_OUT: u8 = 0x01;
pub const ENDPOINT_IN: u8 = 0x81;

/// Bulk endpoints on the Switch DBI interface.
pub struct UsbTransport {
    ep_in: nusb::Endpoint<Bulk, In>,
    ep_out: nusb::Endpoint<Bulk, Out>,
    in_packet_size: usize,
    _interface: Interface,
}

impl UsbTransport {
    /// Wait until the Switch appears, then claim its DBI interface and endpoints.
    pub async fn connect() -> Result<Self, TransportError> {
        loop {
            let info = list_devices()
                .await
                .map_err(|e| TransportError::Usb(e.to_string()))?
                .find(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID);

            let Some(info) = info else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            let device = info
                .open()
                .await
                .map_err(|e| TransportError::Usb(e.to_string()))?;
            let interface = device
                .claim_interface(INTERFACE_NUMBER)
                .await
                .map_err(|e| TransportError::Usb(e.to_string()))?;
            let ep_in = interface
                .endpoint::<Bulk, In>(ENDPOINT_IN)
                .map_err(|e| TransportError::Usb(e.to_string()))?;
            let ep_out = interface
                .endpoint::<Bulk, Out>(ENDPOINT_OUT)
                .map_err(|e| TransportError::Usb(e.to_string()))?;
            let in_packet_size = ep_in.max_packet_size();

            return Ok(Self {
                ep_in,
                ep_out,
                in_packet_size,
                _interface: interface,
            });
        }
    }
}

#[async_trait]
impl Transport for UsbTransport {
    async fn read(&mut self, len: usize) -> Result<Vec<u8>, TransportError> {
        let request = round_up(len, self.in_packet_size);
        let buffer = Buffer::new(request);
        self.ep_in.submit(buffer);
        let buffer = self
            .ep_in
            .next_complete()
            .await
            .into_result()
            .map_err(|e| TransportError::Usb(format!("{e}")))?;
        let data = buffer.into_vec();
        if data.len() < len {
            return Err(TransportError::ShortRead {
                expected: len,
                got: data.len(),
            });
        }
        Ok(data[..len].to_vec())
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        let buffer: Buffer = data.to_vec().into();
        self.ep_out.submit(buffer);
        self.ep_out
            .next_complete()
            .await
            .into_result()
            .map_err(|e| TransportError::Usb(format!("{e}")))?;
        Ok(())
    }
}
