use crate::api::{Message, PROTOCOL_VERSION};
use bytes::Bytes;
use failure::Error;
use futures::select;
use futures::{SinkExt, StreamExt};
use futures_util::future::FutureExt;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub struct Connection {
    pub remote: SocketAddr,
    pub messages_out: Sender<Message>,
    pub messages_in: Receiver<Message>,
}

#[derive(Debug, Fail)]
pub enum HandshakeError {
    #[fail(display = "did not receive a version message")]
    NoVersionReceived,
    #[fail(display = "protocol version mismatch")]
    VersionMismatch,
    #[fail(display = "I/O error :)")]
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
        let (mut tx_encode, rx_encode) = channel::<Bytes>(100);
        let (tx_decode, mut rx_decode) = channel::<Bytes>(100);
        let (tx_message_out, mut rx_message_out) = channel::<Message>(100);
        let (mut tx_message_in, rx_message_in) = channel::<Message>(100);

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
        mut bytes_in: Sender<Bytes>,
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

                    let res = bytes_in.send(in_bytes.unwrap().freeze()).await;
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
