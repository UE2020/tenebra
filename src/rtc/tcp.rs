use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener,
    },
    sync::{
        mpsc::{channel, unbounded_channel, UnboundedReceiver, UnboundedSender},
        Mutex,
    },
};

use log::*;

pub struct Listener {
    tx: UnboundedSender<(Vec<u8>, SocketAddr)>,
    rx: UnboundedReceiver<(Vec<u8>, SocketAddr)>,
}

impl Listener {
    pub fn listen(listener: TcpListener) -> anyhow::Result<Self> {
        // Our TCP candidate is a passive candidate, so we wait
        // for connections from the client's browser.
        // We expose an interface for the rest of the program to
        // wait on messages from all connected peers, and to
        // forward messages to one of the connected peers.
        let (tx, mut task_rx) = unbounded_channel::<(Vec<u8>, SocketAddr)>();
        let (task_tx, rx) = unbounded_channel::<(Vec<u8>, SocketAddr)>();
        tokio::spawn(async move {
            let map = Arc::new(Mutex::new(HashMap::new()));
            loop {
                tokio::select! {
                    // this will stop accepting when task_rx dies
                    Ok((socket, peer_addr)) = listener.accept() => {
                        info!("Accepted TCP connection from {peer_addr}");
                        let (reader, writer) = socket.into_split();
                        let (close_tx, mut close_rx) = channel(1);
                        map.lock().await.insert(peer_addr, (writer, close_tx));
                        let task_tx = task_tx.clone();
                        let map_clone = map.clone();
                        tokio::spawn(async move {
                            tokio::select! {
                                Err(e) = Self::handle_read(task_tx, reader) => {
                                    error!("Failed to read from {peer_addr} because of: {e}");
                                    map_clone.lock().await.remove(&peer_addr);
                                }
                                _ = close_rx.recv() => {}
                            }
                        });
                    }
                    data = task_rx.recv() => {
                        match data {
                            Some((data, addr)) => {
                                let mut map = map.lock().await;
                                if let Some((socket, close_tx)) =  map.get_mut(&addr) {
                                    if let Err(e) = Self::frame_and_send(socket, &data).await {
                                        error!("Closing {addr} because of send error {e}");
                                        close_tx.send(()).await.ok();
                                        map.remove(&addr);
                                    }
                                }
                            },
                            None => {
                                // close every socket
                                let map = map.lock().await;
                                info!("Closing {} sockets.", map.len());
                                for (addr, (_, close_tx)) in map.iter() {
                                    info!("Closing socket {addr}");
                                    close_tx.send(()).await.ok();
                                }

                                break;
                            },
                        }
                    }
                }
            }
        });

        Ok(Self { tx, rx })
    }

    async fn frame_and_send(socket: &mut OwnedWriteHalf, data: &[u8]) -> anyhow::Result<()> {
        socket.write_u16(data.len() as u16).await?;
        socket.write_all(data).await?;
        Ok(())
    }

    pub async fn send(&self, data: &[u8], addr: SocketAddr) -> anyhow::Result<()> {
        self.tx.send((data.to_vec(), addr))?;
        Ok(())
    }

    pub async fn read(&mut self) -> Option<(Vec<u8>, SocketAddr)> {
        self.rx.recv().await
    }

    async fn handle_read(
        tx: UnboundedSender<(Vec<u8>, SocketAddr)>,
        mut reader: OwnedReadHalf,
    ) -> anyhow::Result<()> {
        let peer_addr = reader.peer_addr()?;
        loop {
            let len = reader.read_u16().await?;
            let mut buf = vec![0u8; len as usize];
            reader.read_exact(&mut buf).await?;
            tx.send((buf, peer_addr))?;
        }
    }
}
