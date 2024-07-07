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
