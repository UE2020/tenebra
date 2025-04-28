use std::{os::fd::OwnedFd, sync::OnceLock};

use anyhow::ensure;
use ashpd::desktop::{
    screencast::{CursorMode, Screencast, SourceType, Streams},
    PersistMode,
};

#[derive(Debug)]
pub struct PipewireDisplay {
    pub streams: Streams,
    pub file_descriptor: OwnedFd,
}

pub static STREAMS: OnceLock<PipewireDisplay> = OnceLock::new();

pub async fn setup_wayland_screencast() -> anyhow::Result<()> {
    let proxy = Screencast::new().await?;
    let session = proxy.create_session().await?;
    proxy
        .select_sources(
            &session,
            CursorMode::Embedded,
            SourceType::Monitor | SourceType::Window,
            true,
            None,
            PersistMode::DoNot,
        )
        .await?;

    let response = proxy.start(&session, None).await?.response()?;
    ensure!(response.streams().len() == 1);
    response.streams().iter().for_each(|stream| {
        println!("Got Pipewire stream:");
        println!("\tnode id: {}", stream.pipe_wire_node_id());
        println!("\tsize: {:?}", stream.size());
        println!("\tposition: {:?}", stream.position());
    });

    STREAMS
        .set(PipewireDisplay {
            streams: response,
            file_descriptor: proxy.open_pipe_wire_remote(&session).await.unwrap(),
        })
        .unwrap();

    Ok(())
}
