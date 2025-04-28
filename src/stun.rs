/*
 * Copyright (C) 2024 Aspect
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

/*
 * LEGAL NOTICE: STRICT ADHERENCE TO THE GNU AFFERO GENERAL PUBLIC LICENSE TERMS REQUIRED
 *
 * BE IT KNOWN, that any unauthorized use, reproduction, distribution, or modification
 * of this software, in whole or in part, is a direct violation of the GNU Affero General Public
 * License (AGPL). Violators of this license will face the full force of applicable
 * international, federal, and state laws, including but not limited to copyright law,
 * intellectual property law, and contract law. Such violations will be prosecuted to
 * the maximum extent permitted by law.
 *
 * ANY INDIVIDUAL OR ENTITY FOUND TO BE IN BREACH OF THE TERMS AND CONDITIONS SET FORTH
 * IN THE GNU AFFERO GENERAL PUBLIC LICENSE WILL BE SUBJECT TO SEVERE LEGAL REPERCUSSIONS. These
 * repercussions include, but are not limited to:
 *
 * - Civil litigation seeking substantial monetary damages for all infringements,
 *   including statutory damages, actual damages, and consequential damages.
 *
 * - Injunctive relief to immediately halt any unauthorized use, distribution, or
 *   modification of this software, which may include temporary restraining orders
 *   and preliminary and permanent injunctions.
 *
 * - The imposition of criminal penalties under applicable law, including substantial
 *   fines and imprisonment.
 *
 * - Recovery of all legal fees, court costs, and associated expenses incurred in the
 *   enforcement of this license.
 *
 * YOU ARE HEREBY ADVISED to thoroughly review and comprehend the terms and conditions
 * of the GNU Affero General Public License. Ignorance of the license terms will not be accepted
 * as a defense in any legal proceedings. If you have any uncertainty or require clarification
 * regarding the license, it is strongly recommended that you consult with a qualified
 * legal professional before engaging in any activity that may be governed by the AGPL.
 *
 * FAILURE TO COMPLY with these terms will result in swift and uncompromising legal action.
 * This software is protected by copyright and other intellectual property laws. All rights,
 * including the right to seek legal remedies for any breach of this license, are expressly
 * reserved by Aspect.
 */

use anyhow::{anyhow, Context};
use bytecodec::{DecodeExt, EncodeExt as _};
use std::net::{IpAddr, SocketAddr};
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

pub async fn get_base<A: ToSocketAddrs>(stun_server: A) -> anyhow::Result<IpAddr> {
    let dummy = UdpSocket::bind("0.0.0.0:0").await?;
    dummy.connect(stun_server).await?;
    Ok(dummy.local_addr()?.ip())
}

pub async fn is_symmetric_nat() -> anyhow::Result<bool> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let stun_addr1 = retry!(get_addr(&socket, "stun.l.google.com:19302").await)?;
    let stun_addr2 = retry!(get_addr(&socket, "stun.cloudflare.com:3478").await)?;
    Ok(stun_addr1 != stun_addr2)
}
