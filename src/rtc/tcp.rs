use std::{collections::HashMap, net::SocketAddr};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{tcp::OwnedReadHalf, TcpListener},
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};

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
            let mut map = HashMap::new();
            loop {
                tokio::select! {
                    Ok((socket, peer_addr)) = listener.accept() => {
                        println!("Accepted TCP connection from {peer_addr}");
                        let (reader, writer) = socket.into_split();
                        map.insert(peer_addr, writer);
                        tokio::spawn(Self::handle_read(task_tx.clone(), reader));
                    }
                    data = task_rx.recv() => {
                        match data {
                            Some((data, addr)) => {
                                if let Some(socket) =  map.get_mut(&addr) {
                                    socket.write_u16(data.len() as u16).await.ok();
                                    socket.write_all(data.as_slice()).await.ok();
                                }
                            },
                            None => break,
                        }
                    }
                }
            }
        });

        Ok(Self { tx, rx })
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
