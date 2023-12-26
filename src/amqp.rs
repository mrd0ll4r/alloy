use crate::event::AddressedEvent;
use anyhow::{bail, Context, Result};
use futures::{Stream, StreamExt};
use lapin::message::Delivery;
use lapin::options::{
    BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, BasicQosOptions,
    ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
};
use lapin::types::{FieldTable, ShortString};
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties, Consumer, ExchangeKind};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::str::FromStr;
use std::task::Poll;
use tokio::sync::mpsc::{Receiver, Sender};

pub const EXCHANGE_NAME_SUBMARINE_INPUT: &str = "submarine.input";
pub const EXCHANGE_NAME_LOGGING: &str = "logging";

pub const ROUTING_PREFIX_ALIAS: &str = "alias";
pub const ROUTING_PREFIX_APPLICATION: &str = "application";

#[derive(Debug)]
pub struct ExchangeSubmarineInput {
    client: Client,
}

impl ExchangeSubmarineInput {
    pub async fn new(
        addr: &str,
        subscriptions: &[RoutingKeySubscription<SubmarineInputRoutingKey>],
    ) -> Result<ExchangeSubmarineInput> {
        let client: Client = Client::new(
            addr,
            ExchangeParameters::SubmarineInput,
            subscriptions
                .iter()
                .map(|k| k.to_routing_key())
                .collect::<Vec<_>>()
                .as_slice(),
        )
        .await
        .context("unable to connect to set up AMQP client")?;

        Ok(ExchangeSubmarineInput { client })
    }

    pub async fn new_publisher(
        &self,
        routing_key: SubmarineInputRoutingKey,
    ) -> Result<ExchangeSubmarineInputPublisher> {
        let publisher = self
            .client
            .new_publisher()
            .await
            .context("unable to set up publisher")?;

        Ok(ExchangeSubmarineInputPublisher {
            publisher,
            routing_key: routing_key.to_routing_key(),
        })
    }
}

impl Stream for ExchangeSubmarineInput {
    type Item = Result<(SubmarineInputRoutingKey, AddressedEvent)>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.client.poll_next_unpin(cx).map(|res| {
            res.map(|res| {
                res.and_then(|(key, data)| {
                    let routing_key = match SubmarineInputRoutingKey::from_str(&key) {
                        Ok(k) => k,
                        Err(e) => return Err(e.context("unable to decode routing key").into()),
                    };
                    let msg = match decode_message::<AddressedEvent>(&data) {
                        Ok(msg) => msg,
                        Err(e) => return Err(e.context("unable to decode message").into()),
                    };
                    Ok((routing_key, msg))
                })
            })
        })
    }
}

#[derive(Debug)]
pub struct ExchangeSubmarineInputPublisher {
    publisher: Publisher,
    routing_key: String,
}

impl ExchangeSubmarineInputPublisher {
    pub async fn publish_event(&self, event: &AddressedEvent) -> Result<()> {
        let payload = encode_messages(&event).context("unable to encode message")?;
        self.publisher
            .post_message(&self.routing_key, &payload)
            .await
            .context("unable to post message")?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum RoutingKeySubscription<T> {
    Selective(T),
    All,
}

impl<T: RoutingKey> RoutingKeySubscription<T> {
    fn to_routing_key(&self) -> String {
        match self {
            RoutingKeySubscription::Selective(s) => s.to_routing_key(),
            RoutingKeySubscription::All => "#".to_string(),
        }
    }
}

pub trait RoutingKey: Sized {
    fn to_routing_key(&self) -> String;
    fn from_str(key: &str) -> Result<Self>;
}

#[derive(Clone, Debug)]
pub struct SubmarineInputRoutingKey {
    pub alias: String,
}

impl SubmarineInputRoutingKey {
    pub fn from_alias(alias: String) -> SubmarineInputRoutingKey {
        SubmarineInputRoutingKey { alias }
    }
}

impl RoutingKey for SubmarineInputRoutingKey {
    fn to_routing_key(&self) -> String {
        format!("{}.{}", ROUTING_PREFIX_ALIAS, self.alias)
    }

    fn from_str(key: &str) -> Result<Self> {
        let split: Vec<_> = key.split('.').collect();
        if split.len() != 2 {
            bail!("expected 2 parts, found {}", split.len());
        }
        if *split.get(0).unwrap() != ROUTING_PREFIX_ALIAS {
            bail!(
                "expected prefix {}, found {}",
                ROUTING_PREFIX_ALIAS,
                split.get(0).unwrap()
            );
        }

        Ok(SubmarineInputRoutingKey {
            alias: split.get(1).unwrap().to_string(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct LoggingRoutingKey {
    pub application: String,
    pub module: String,
    pub level: log::Level,
}

impl RoutingKey for LoggingRoutingKey {
    fn to_routing_key(&self) -> String {
        format!(
            "{}.{}.{}.{}",
            ROUTING_PREFIX_APPLICATION,
            self.application,
            self.module,
            self.level.as_str()
        )
    }

    fn from_str(key: &str) -> Result<Self> {
        let split: Vec<_> = key.split('.').collect();
        if split.len() != 4 {
            bail!("expected 4 parts, found {}", split.len());
        }
        if *split.get(0).unwrap() != ROUTING_PREFIX_APPLICATION {
            bail!(
                "expected prefix {}, found {}",
                ROUTING_PREFIX_APPLICATION,
                split.get(0).unwrap()
            );
        }

        let level =
            log::Level::from_str(split.get(3).unwrap()).context("unable to decode log level")?;

        Ok(LoggingRoutingKey {
            level,
            application: split.get(1).unwrap().to_string(),
            module: split.get(2).unwrap().to_string(),
        })
    }
}

#[derive(Clone, Debug)]
enum ExchangeParameters {
    SubmarineInput,
    Logging,
}

impl ExchangeParameters {
    fn name(&self) -> &str {
        match self {
            ExchangeParameters::SubmarineInput => EXCHANGE_NAME_SUBMARINE_INPUT,
            ExchangeParameters::Logging => EXCHANGE_NAME_LOGGING,
        }
    }

    fn message_ttl(&self) -> ShortString {
        match self {
            ExchangeParameters::SubmarineInput => {
                // 10 seconds
                ShortString::from("10000")
            }
            ExchangeParameters::Logging => {
                // 60 seconds
                ShortString::from("60000")
            }
        }
    }

    fn durable(&self) -> bool {
        match self {
            ExchangeParameters::SubmarineInput => false,
            ExchangeParameters::Logging => true,
        }
    }
}

async fn connect(addr: &str) -> Result<Connection> {
    let conn = Connection::connect(addr, ConnectionProperties::default())
        .await
        .context("unable to connect")?;
    Ok(conn)
}

async fn set_up_exchanges(c: &Channel) -> Result<()> {
    let exchanges = vec![
        ExchangeParameters::Logging,
        ExchangeParameters::SubmarineInput,
    ];

    for p in exchanges {
        c.exchange_declare(
            p.name(),
            ExchangeKind::Topic,
            ExchangeDeclareOptions {
                passive: false,
                durable: p.durable(),
                auto_delete: false,
                internal: false,
                nowait: false,
            },
            FieldTable::default(),
        )
        .await
        .context(format!("unable to set up exchange {}", p.name()))?;
    }

    Ok(())
}

async fn publish_message(
    c: &Channel,
    exchange: &ExchangeParameters,
    routing_key: &str,
    payload: &[u8],
) -> Result<()> {
    c.basic_publish(
        exchange.name(),
        &routing_key,
        BasicPublishOptions {
            // Does not need to be routed anywhere (i.e., no subscribers?)
            mandatory: false,
            // Does not need to be routed immediately (i.e., backpressure? no subscribers?)
            immediate: false,
        },
        payload,
        BasicProperties::default()
            .with_expiration(exchange.message_ttl())
            .with_delivery_mode(if exchange.durable() { 2 } else { 1 }),
    )
    .await
    .context("unable to basic.publish")?;
    Ok(())
}

async fn set_up_queue_and_subscribe(
    c: &Channel,
    exchange: &ExchangeParameters,
    routing_keys: &[String],
) -> Result<Consumer> {
    let queue = c
        .queue_declare(
            "",
            QueueDeclareOptions {
                exclusive: true,
                durable: false,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await
        .context("unable to declare queue")?;

    let queue_name = queue.name();
    for routing_key in routing_keys {
        c.queue_bind(
            queue_name.as_str(),
            exchange.name(),
            routing_key,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .context(format!("unable to bind with routing key {}", routing_key))?;
    }

    let consumer = c
        .basic_consume(
            queue_name.as_str(),
            "",
            BasicConsumeOptions {
                no_local: false,
                no_ack: false,
                exclusive: true,
                nowait: false,
            },
            FieldTable::default(),
        )
        .await
        .context("unable to basic.consume")?;

    Ok(consumer)
}

fn decode_message<'a, T: Deserialize<'a>>(payload: &'a [u8]) -> Result<T> {
    debug!("decoding {} bytes: {:x?}", payload.len(), payload);
    serde_json::from_slice(payload).map_err(|err| err.into())
}

fn encode_messages<T: Serialize>(msg: &T) -> Result<Vec<u8>> {
    let b = serde_json::to_vec(msg)?;
    debug!("encoded {} bytes: {:x?}", b.len(), b);
    Ok(b)
}

#[derive(Debug)]
struct Publisher {
    chan: Channel,
    exchange: ExchangeParameters,
}

impl Publisher {
    pub async fn post_message(&self, key: &str, msg: &[u8]) -> Result<()> {
        publish_message(&self.chan, &self.exchange, key, msg).await?;
        Ok(())
    }
}

#[derive(Debug)]
struct Client {
    conn: Connection,
    exchange: ExchangeParameters,
    msg_in: Receiver<Result<(String, Vec<u8>)>>,
}

impl Stream for Client {
    type Item = Result<(String, Vec<u8>)>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.msg_in.poll_recv(cx)
    }
}

impl Client {
    async fn new(
        addr: &str,
        exchange: ExchangeParameters,
        routing_keys: &[String],
    ) -> Result<Client> {
        let conn = connect(addr)
            .await
            .context("unable to connect to RabbitMQ")?;

        let chan = conn
            .create_channel()
            .await
            .context("unable to set up AMQP channel")?;

        chan.basic_qos(10, BasicQosOptions::default())
            .await
            .context("unable to set prefetch")?;

        set_up_exchanges(&chan)
            .await
            .context("unable to set up exchange")?;

        let consumer = set_up_queue_and_subscribe(&chan, &exchange, &routing_keys)
            .await
            .context("unable to set up queue and subscribe")?;

        let (msg_sender, msg_receiver) = tokio::sync::mpsc::channel(1);

        tokio::spawn(Self::process_incoming_messages(consumer, msg_sender));

        Ok(Client {
            conn,
            exchange,
            msg_in: msg_receiver,
        })
    }

    async fn new_publisher(&self) -> Result<Publisher> {
        let chan = self
            .conn
            .create_channel()
            .await
            .context("unable to set up AMQP channel")?;

        Ok(Publisher {
            chan,
            exchange: self.exchange.clone(),
        })
    }

    async fn process_incoming_messages(
        mut consumer: Consumer,
        msg_out: Sender<Result<(String, Vec<u8>)>>,
    ) {
        while let Some(delivery) = consumer.next().await {
            match delivery {
                Err(err) => {
                    // We ignore this error because we return immediately.
                    let _ = msg_out.send(Err(err.into())).await;
                    return;
                }
                Ok(delivery) => {
                    debug!("got delivery {:?}", delivery);

                    // Destructure
                    let Delivery {
                        data,
                        routing_key,
                        acker,
                        ..
                    } = delivery;

                    // Pass on to the application, then ACK.
                    if let Err(_) = msg_out.send(Ok((routing_key.to_string(), data))).await {
                        debug!("unable to pass on received message, quitting");
                        return;
                    }

                    if let Err(e) = acker.ack(BasicAckOptions::default()).await {
                        // This probably means something is wrong, so let's abort.
                        error!("unable to ACK incoming delivery: {:?}", e);
                        if let Err(e) = msg_out.send(Err(e.into())).await {
                            error!("unable to notify subscriber of error: {:?}", e);
                        }
                        return;
                    }
                }
            }
        }
    }
}
