use crate::api::{
    APIRequest, APIResult, Message, SetRequest, SubscriptionRequest, PROTOCOL_VERSION,
};
use crate::config::UniverseConfig;
use crate::event::AddressedEvent;
use anyhow::{bail, Context, Error};
use bytes::Bytes;
use futures::select;
use futures::{SinkExt, StreamExt};
use futures_util::future::FutureExt;
use log::{debug, error};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::{oneshot, Mutex};
use tokio::task;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

/// A synchronous TCP client.
/// Runs a tokio runtime internally.
#[derive(Debug, Clone)]
pub struct SyncClient {
    rt: Arc<tokio::runtime::Runtime>,
    client: Arc<AsyncClient>,
}

impl SyncClient {
    pub fn new<A: std::net::ToSocketAddrs>(
        addr: A,
    ) -> Result<(SyncClient, std::sync::mpsc::Receiver<PushedMessage>), Error> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .build()
            .context("unable to create tokio executor")?;

        let addrs: Vec<_> = addr
            .to_socket_addrs()
            .context("unable to resolve address")?
            .collect();

        let (client, mut push_msg_stream) = rt
            .block_on(AsyncClient::new(addrs.as_slice()))
            .context("unable to create client")?;

        let (tx, rx) = std::sync::mpsc::channel();

        rt.spawn(async move {
            debug!("started push messages async->sync task");
            while let Some(msg) = push_msg_stream.recv().await {
                debug!("got pushed message {:?}", msg);
                match tx.send(msg) {
                    Ok(_) => {}
                    Err(e) => {
                        debug!(
                            "unable to send on push message channel: {:?}, will close",
                            e
                        );
                        break;
                    }
                }
            }
            debug!("quit push message async->sync task");
        });

        Ok((
            SyncClient {
                rt: Arc::new(rt),
                client: Arc::new(client),
            },
            rx,
        ))
    }

    pub fn ping(&self) -> Result<(), Error> {
        self.rt.block_on(self.client.ping())
    }

    pub fn subscribe(&self, req: SubscriptionRequest) -> Result<(), Error> {
        self.rt.block_on(self.client.subscribe(req))
    }

    pub fn set(&self, req: Vec<SetRequest>) -> Result<(), Error> {
        self.rt.block_on(self.client.set(req))
    }
}

// An asynchronous TCP client.
// Needs an executor to be run.
#[derive(Debug)]
pub struct AsyncClient {
    messages_out: Sender<Message>,
    inner: Arc<Mutex<ClientInner>>,
}

#[derive(Debug)]
struct ClientInner {
    results: Vec<Option<oneshot::Sender<Result<APIResult, Error>>>>,
    current_id: u16,
}

#[derive(Debug, Clone)]
pub enum PushedMessage {
    Event(AddressedEvent),
    Config(UniverseConfig),
}

impl AsyncClient {
    pub async fn new<A: ToSocketAddrs>(
        addr: A,
    ) -> Result<(AsyncClient, Receiver<PushedMessage>), Error> {
        let conn = TcpStream::connect(addr).await?;

        Self::new_from_conn(conn).await
    }

    pub async fn new_from_conn(
        conn: TcpStream,
    ) -> Result<(AsyncClient, Receiver<PushedMessage>), Error> {
        conn.set_nodelay(true)?;
        let conn = Connection::new(conn).await?;

        let messages_out = conn.messages_out.clone();
        let messages_in = conn.messages_in;
        let (push_messages_tx, push_messages_rx) = channel::<PushedMessage>(100);

        let mut ci = ClientInner {
            results: Vec::with_capacity(4096),
            current_id: 0,
        };
        for _i in 0..4096 {
            ci.results.push(None);
        }

        let inner = Arc::new(Mutex::new(ci));
        let inner2 = inner.clone();

        task::spawn(Self::handle_incoming_messages(
            inner2,
            messages_in,
            push_messages_tx,
        ));

        Ok((
            AsyncClient {
                messages_out,
                inner,
            },
            push_messages_rx,
        ))
    }

    async fn handle_incoming_messages(
        inner: Arc<Mutex<ClientInner>>,
        mut messages_in: Receiver<Message>,
        push_messages_out: Sender<PushedMessage>,
    ) {
        while let Some(msg) = messages_in.recv().await {
            debug!("received message: {:?}", msg);
            match msg {
                Message::Version(_) => {
                    error!("received unexpected version message");
                    break;
                }
                Message::Events(events) => {
                    for event in events {
                        let res = push_messages_out.send(PushedMessage::Event(event)).await;
                        match res {
                            Err(e) => {
                                error!("unable to push event: {:?}", e);
                                break;
                            }
                            Ok(()) => {}
                        }
                    }
                }
                Message::Request { id: _, inner: _ } => {
                    error!("received unexpected request message");
                    break;
                }
                Message::Response { id, inner: res } => {
                    debug!("got response {} => {:?}", id, res);
                    let mut inner = inner.lock().await;
                    let receiver = inner.results[id as usize].take();
                    if receiver.is_none() {
                        error!("received response without request? {} => {:?}", id, res);
                        break;
                    }

                    receiver
                        .unwrap()
                        .send(res.map_err(|e| anyhow::anyhow!(e)))
                        .unwrap();
                }
                Message::Config(cfg) => {
                    let res = push_messages_out.send(PushedMessage::Config(cfg)).await;
                    match res {
                        Err(e) => {
                            error!("unable to push universe config: {:?}", e);
                            break;
                        }
                        Ok(()) => {}
                    }
                }
            }
        }

        debug!("handler shutting down")
    }

    async fn perform_request(&self, req: APIRequest) -> Result<APIResult, Error> {
        let (id, receiver) = {
            let mut inner = self.inner.lock().await;
            let next_id = (inner.current_id + 1) % 4096;
            if inner.results[next_id as usize].is_some() {
                // TODO wait
                bail!("too many open requests")
            }

            inner.current_id = next_id;
            let (tx, rx) = oneshot::channel();
            inner.results[next_id as usize] = Some(tx);
            (next_id, rx)
        };

        self.messages_out
            .clone()
            .send(Message::Request { id, inner: req })
            .await?;

        receiver.await.unwrap()
    }

    pub async fn ping(&self) -> Result<(), Error> {
        let res = self.perform_request(APIRequest::Ping).await?;
        match res {
            APIResult::Ping => Ok(()),
            _ => {
                // what do?
                panic!("received invalid response, expected Ping, got {:?}", res)
            }
        }
    }

    pub async fn set(&self, req: Vec<SetRequest>) -> Result<(), Error> {
        let res = self.perform_request(APIRequest::Set(req)).await?;
        match res {
            APIResult::Set => Ok(()),
            _ => {
                // what do?
                panic!("received invalid response, expected Set, got {:?}", res)
            }
        }
    }

    pub async fn subscribe(&self, req: SubscriptionRequest) -> Result<(), Error> {
        let res = self.perform_request(APIRequest::Subscribe(req)).await?;
        match res {
            APIResult::Subscribe => Ok(()),
            _ => {
                // what do?
                panic!(
                    "received invalid response, expected Subscribe, got {:?}",
                    res
                )
            }
        }
    }
}

/// A low-level wrapper for a TCP connection.
/// This essentially encodes and decodes messages.
#[derive(Debug)]
pub struct Connection {
    pub remote: SocketAddr,
    pub messages_out: Sender<Message>,
    pub messages_in: Receiver<Message>,
}

/// Errors during the handshake procedure.
#[derive(Debug, thiserror::Error)]
pub enum HandshakeError {
    #[error("did not receive a version message")]
    NoVersionReceived,
    #[error("protocol version mismatch")]
    VersionMismatch,
    #[error("I/O error :)")]
    IO,
}

impl Connection {
    pub async fn new(conn: TcpStream) -> Result<Connection, Error> {
        let remote = conn.peer_addr()?;
        // Set up length-delimited frames
        let mut framed = Framed::new(
            conn,
            LengthDelimitedCodec::builder()
                .length_field_length(2)
                .new_codec(),
        );

        // Exchange version
        Self::ensure_version(&mut framed).await?;

        // Set up some plumbing...
        let (tx_encode, rx_encode) = channel::<Bytes>(100);
        let (tx_decode, mut rx_decode) = channel::<Bytes>(100);
        let (tx_message_out, mut rx_message_out) = channel::<Message>(100);
        let (tx_message_in, rx_message_in) = channel::<Message>(100);

        // Decode incoming messages
        task::spawn(async move {
            while let Some(buf) = rx_decode.recv().await {
                debug!("decoder {}: got buffer {:?}", remote, buf);

                let res = serde_json::from_slice(&buf);
                if let Err(e) = res {
                    error!("decoder {}: unable to decode: {:?}", remote, e);
                    break;
                }
                let msg = res.unwrap();
                debug!("decoder {}: decoded {:?}", remote, msg);

                let res = tx_message_in.send(msg).await;
                if let Err(e) = res {
                    debug!("decoder {}: unable to send: {:?}", remote, e);
                    break;
                }
            }

            debug!("decoder {}: shutting down", remote);
        });

        // Encode outgoing messages
        task::spawn(async move {
            while let Some(msg) = rx_message_out.recv().await {
                debug!("encoder {}: got message {:?}", remote, msg);

                let buf = serde_json::to_vec(&msg).expect("serialization failed");

                let res = tx_encode.send(Bytes::from(buf)).await;
                if let Err(e) = res {
                    debug!("encoder {}: unable to send: {:?}", remote, e);
                    break;
                }
            }

            debug!("encoder {}: shutting down", remote);
        });

        // Handle socket I/O
        task::spawn(Self::handle_socket_io(remote, framed, tx_decode, rx_encode));

        Ok(Connection {
            remote,
            messages_out: tx_message_out,
            messages_in: rx_message_in,
        })
    }

    async fn handle_socket_io(
        remote: SocketAddr,
        mut framed: Framed<TcpStream, LengthDelimitedCodec>,
        bytes_in: Sender<Bytes>,
        mut bytes_out: Receiver<Bytes>,
    ) {
        loop {
            select! {
                in_bytes = framed.next().fuse() => {
                    if let None = in_bytes {
                        debug!("I/O {}: incoming connection closed",remote);
                        break;
                    }
                    let in_bytes = in_bytes.unwrap();
                    if let Err(e) = in_bytes {
                        error!("I/O {}: socket read error: {:?}",remote,e);
                        break;
                    }
                    let in_bytes = in_bytes.unwrap();
                    debug!("I/O {}: got bytes: {:?}",remote,in_bytes);

                    let res = bytes_in.send(in_bytes.freeze()).await;
                    if let Err(e) = res {
                        // This can only happen if the decoder shut down, i.e. we're dropping the
                        // client.
                        debug!("I/O {}: unable to send to decoder: {:?}",remote,e);
                        break;
                    }
                },
                out_bytes = bytes_out.recv().fuse() => {
                    if let None = out_bytes {
                        debug!("I/O {}: outgoing byte stream closed",remote);
                        break;
                    }
                    let res = framed.send(out_bytes.unwrap()).await;
                    if let Err(e) = res {
                        // TODO handle backpressure? Maybe not because single producer? Maybe not
                        // because tokio channels behave differently on send?
                        error!("I/O {}: unable to send to socket: {:?}",remote,e);
                        break;
                    }
                }
            }
        }

        debug!("I/O {}: shutting down", remote);
    }

    async fn ensure_version(
        framed: &mut Framed<TcpStream, LengthDelimitedCodec>,
    ) -> Result<(), HandshakeError> {
        let version_msg = Message::Version(PROTOCOL_VERSION);
        let version_msg_bytes = serde_json::to_vec(&version_msg).expect("unable to serialize");
        framed
            .send(Bytes::from(version_msg_bytes))
            .await
            .map_err(|_| HandshakeError::IO)?;

        let rec = framed.next().await;
        if rec.is_none() {
            return Err(HandshakeError::NoVersionReceived);
        }
        let rec = rec.unwrap();
        if let Err(_e) = rec {
            return Err(HandshakeError::IO);
        }
        let rec = rec.unwrap();
        let remote_version_msg: Message =
            serde_json::from_slice(rec.as_ref()).map_err(|_| HandshakeError::IO)?;
        match remote_version_msg {
            Message::Version(v) => {
                if v != PROTOCOL_VERSION {
                    return Err(HandshakeError::VersionMismatch);
                }
            }
            _ => return Err(HandshakeError::NoVersionReceived),
        }

        Ok(())
    }
}
