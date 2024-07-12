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

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;

use tokio::sync::Mutex;

use webrtc::interceptor::{Attributes, RTPWriter, *};

use async_trait::async_trait;

pub(super) struct SenderStream {
    next_rtp_writer: Arc<dyn RTPWriter + Send + Sync>,
    hdr_ext_id: u8,
}

impl SenderStream {
    pub(super) fn new(next_rtp_writer: Arc<dyn RTPWriter + Send + Sync>, hdr_ext_id: u8) -> Self {
        SenderStream {
            next_rtp_writer,
            hdr_ext_id,
        }
    }
}

#[async_trait]
impl RTPWriter for SenderStream {
    async fn write(
        &self,
        pkt: &webrtc::rtp::packet::Packet,
        a: &Attributes,
    ) -> Result<usize, webrtc::interceptor::Error> {
        let mut pkt = pkt.clone();
        pkt.header
            .set_extension(self.hdr_ext_id, Bytes::copy_from_slice(&[0x00, 0x00, 0x00]))?;

        self.next_rtp_writer.write(&pkt, a).await
    }
}

pub(crate) const PLAYOUT_DELAY_URI: &str =
    "http://www.webrtc.org/experiments/rtp-hdrext/playout-delay";

#[derive(Default)]
pub struct SenderBuilder {}

impl InterceptorBuilder for SenderBuilder {
    fn build(
        &self,
        _id: &str,
    ) -> Result<Arc<dyn Interceptor + Send + Sync>, webrtc::interceptor::Error> {
        Ok(Arc::new(Sender {
            streams: Mutex::new(HashMap::new()),
        }))
    }
}

pub struct Sender {
    streams: Mutex<HashMap<u32, Arc<SenderStream>>>,
}

impl Sender {
    pub fn builder() -> SenderBuilder {
        SenderBuilder::default()
    }
}

#[async_trait]
impl Interceptor for Sender {
    async fn bind_rtcp_reader(
        &self,
        reader: Arc<dyn RTCPReader + Send + Sync>,
    ) -> Arc<dyn RTCPReader + Send + Sync> {
        reader
    }

    async fn bind_rtcp_writer(
        &self,
        writer: Arc<dyn RTCPWriter + Send + Sync>,
    ) -> Arc<dyn RTCPWriter + Send + Sync> {
        writer
    }

    async fn bind_local_stream(
        &self,
        info: &webrtc::interceptor::stream_info::StreamInfo,
        writer: Arc<dyn RTPWriter + Send + Sync>,
    ) -> Arc<dyn RTPWriter + Send + Sync> {
        let mut hdr_ext_id = 0u8;
        for e in &info.rtp_header_extensions {
            if e.uri == PLAYOUT_DELAY_URI {
                hdr_ext_id = e.id as u8;
                break;
            }
        }
        if hdr_ext_id == 0 {
            // Don't add header extension if ID is 0, because 0 is an invalid extension ID
            return writer;
        }

        let stream = Arc::new(SenderStream::new(writer, hdr_ext_id));

        {
            let mut streams = self.streams.lock().await;
            streams.insert(info.ssrc, Arc::clone(&stream));
        }

        stream
    }

    async fn unbind_local_stream(&self, info: &webrtc::interceptor::stream_info::StreamInfo) {
        let mut streams = self.streams.lock().await;
        streams.remove(&info.ssrc);
    }

    async fn bind_remote_stream(
        &self,
        _info: &webrtc::interceptor::stream_info::StreamInfo,
        reader: Arc<dyn RTPReader + Send + Sync>,
    ) -> Arc<dyn RTPReader + Send + Sync> {
        reader
    }

    async fn unbind_remote_stream(&self, _info: &webrtc::interceptor::stream_info::StreamInfo) {}

    async fn close(&self) -> Result<(), webrtc::interceptor::Error> {
        Ok(())
    }
}
