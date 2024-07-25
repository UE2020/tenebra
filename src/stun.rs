use anyhow::{anyhow, Context};
use bytecodec::{DecodeExt, EncodeExt as _};
use std::net::SocketAddr;
use stun_codec::rfc5389::{attributes::XorMappedAddress, methods::BINDING, Attribute};
use stun_codec::*;
use tokio::net::{ToSocketAddrs, UdpSocket};

fn make_binding_request() -> anyhow::Result<Vec<u8>> {
    let request = Message::<Attribute>::new(
        MessageClass::Request,
        BINDING,
        TransactionId::new(rand::random()),
    );

    Ok(MessageEncoder::<Attribute>::default().encode_into_bytes(request)?)
}

fn parse_binding_response(buf: &[u8]) -> anyhow::Result<SocketAddr> {
    let message = MessageDecoder::<Attribute>::default()
        .decode_from_bytes(buf)?
        .map_err(|_| anyhow!("Broken message"))?;

    Ok(message
        .get_attribute::<XorMappedAddress>()
        .context("XOR mapped address not present")?
        .address())
}

#[macro_export]
macro_rules! retry {
    ($f:expr, $count:expr, $interval:expr) => {{
        let mut retries = 0;
        let result = loop {
            let result = $f;
            if result.is_ok() {
                break result;
            } else if retries > $count {
                break result;
            } else {
                retries += 1;
                tokio::time::sleep(std::time::Duration::from_millis($interval)).await;
            }
        };
        result
    }};
    ($f:expr) => {
        retry!($f, 5, 100)
    };
}

pub async fn get_addr<A: ToSocketAddrs>(
    socket: &UdpSocket,
    stun_server: A,
) -> anyhow::Result<SocketAddr> {
    socket
        .send_to(&make_binding_request()?, stun_server)
        .await?;

    let mut buf = vec![0u8; 100];
    let num_read = socket.recv(&mut buf).await?;
    let address = parse_binding_response(&buf[..num_read])?;

    Ok(address)
}

pub async fn is_symmetric_nat() -> anyhow::Result<bool> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let stun_addr1 = retry!(get_addr(&socket, "stun.l.google.com:19302").await)?;
    let stun_addr2 = retry!(get_addr(&socket, "stun.cloudflare.com:3478").await)?;
    Ok(stun_addr1 != stun_addr2)
}
