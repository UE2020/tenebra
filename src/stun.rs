use bytecodec::{DecodeExt, EncodeExt as _};
use std::net::SocketAddr;
use stun_codec::rfc5389::{attributes::XorMappedAddress, methods::BINDING, Attribute};
use stun_codec::*;
use tokio::net::UdpSocket;

fn make_binding_request() -> Vec<u8> {
    let request = Message::<Attribute>::new(
        MessageClass::Request,
        BINDING,
        TransactionId::new(rand::random()),
    );

    MessageEncoder::<Attribute>::default()
        .encode_into_bytes(request)
        .unwrap()
}

fn parse_binding_response(buf: &[u8]) -> SocketAddr {
    let message = MessageDecoder::<Attribute>::default()
        .decode_from_bytes(buf)
        .unwrap()
        .unwrap();

    message
        .get_attribute::<XorMappedAddress>()
        .unwrap()
        .address()
}

pub async fn get_addr(socket: &UdpSocket) -> anyhow::Result<SocketAddr> {
    socket
        .send_to(&make_binding_request(), "stun.l.google.com:19302")
        .await?;

    let mut buf = vec![0u8; 100];
    let num_read = socket.recv(&mut buf).await?;
    let address = parse_binding_response(&buf[..num_read]);

    println!("Our public IP is: {address}");

    Ok(address)
}
