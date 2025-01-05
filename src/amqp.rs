use crate::config::InputValueType;
use crate::event::AddressedEvent;
use anyhow::{bail, ensure, Context, Result};
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

pub const EXCHANGE_NAME_SHACK_INPUT: &str = "shack.input";
pub const EXCHANGE_NAME_SHACK_MQTT: &str = "shack.mqtt";
pub const EXCHANGE_NAME_LOGGING: &str = "logging";

pub const ROUTING_PREFIX_ALIAS: &str = "alias";
pub const ROUTING_PREFIX_TYPE: &str = "type";
pub const ROUTING_PREFIX_APPLICATION: &str = "application";

// Prefixes for Tasmota MQTT routing keys.
pub const ROUTING_PREFIX_TASMOTA_STAT: &str = "stat";
pub const ROUTING_PREFIX_TASMOTA_TELE: &str = "tele";

// Routing key segments for Tasmota MQTT routing keys.
pub const ROUTING_KEY_TASMOTA_SENSOR: &str = "SENSOR";
pub const ROUTING_KEY_TASMOTA_STATE: &str = "STATE";
pub const ROUTING_KEY_TASMOTA_RESULT: &str = "RESULT";
pub const ROUTING_KEY_TASMOTA_LWT: &str = "LWT";
pub const ROUTING_KEY_TASMOTA_INFO1: &str = "INFO1";
pub const ROUTING_KEY_TASMOTA_INFO2: &str = "INFO2";
pub const ROUTING_KEY_TASMOTA_INFO3: &str = "INFO3";

pub const ROUTING_KEY_TEMPERATURE: &str = "temperature";
pub const ROUTING_KEY_HUMIDITY: &str = "humidity";
pub const ROUTING_KEY_BINARY: &str = "binary";
pub const ROUTING_KEY_PRESSURE: &str = "pressure";
pub const ROUTING_KEY_GAS: &str = "gas";
pub const ROUTING_KEY_CONTINUOUS: &str = "continuous";

#[derive(Debug)]
pub struct ExchangeShackInput {
    client: Client,
}

impl ExchangeShackInput {
    pub async fn new(
        addr: &str,
        subscriptions: &[RoutingKeySubscription<ShackInputRoutingKey>],
    ) -> Result<ExchangeShackInput> {
        let client: Client = Client::new(
            addr,
            ExchangeParameters::ShackInput,
            subscriptions
                .iter()
                .map(|k| k.to_routing_key())
                .collect::<Vec<_>>()
                .as_slice(),
        )
        .await
        .context("unable to connect to set up AMQP client")?;

        Ok(ExchangeShackInput { client })
    }

    pub async fn new_publisher(
        &self,
        routing_key: ShackInputRoutingKey,
    ) -> Result<ExchangeShackInputPublisher> {
        let publisher = self
            .client
            .new_publisher()
            .await
            .context("unable to set up publisher")?;

        Ok(ExchangeShackInputPublisher {
            publisher,
            routing_key: routing_key.to_routing_key(),
        })
    }
}

impl Stream for ExchangeShackInput {
    type Item = Result<(ShackInputRoutingKey, AddressedEvent)>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.client.poll_next_unpin(cx).map(|res| {
            res.map(|res| {
                res.and_then(|(key, data)| {
                    let routing_key = match ShackInputRoutingKey::from_str(&key) {
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
pub struct ExchangeShackInputPublisher {
    publisher: Publisher,
    routing_key: String,
}

impl ExchangeShackInputPublisher {
    pub async fn publish_event(&self, event: &AddressedEvent) -> Result<()> {
        self.publish_raw(&self.routing_key, event).await
    }

    pub async fn publish_raw(&self, routing_key: &str, event: &AddressedEvent) -> Result<()> {
        let payload = encode_messages(&event).context("unable to encode message")?;
        self.publisher
            .post_message(routing_key, &payload)
            .await
            .context("unable to post message")?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ExchangeShackMQTT {
    client: Client,
}

impl ExchangeShackMQTT {
    pub async fn new(
        addr: &str,
        subscriptions: &[RoutingKeySubscription<ShackMQTTRoutingKey>],
    ) -> Result<Self> {
        let client: Client = Client::new(
            addr,
            ExchangeParameters::ShackMQTT,
            subscriptions
                .iter()
                .map(|k| k.to_routing_key())
                .collect::<Vec<_>>()
                .as_slice(),
        )
        .await
        .context("unable to connect to set up AMQP client")?;

        Ok(ExchangeShackMQTT { client })
    }

    /*
    pub async fn new_publisher(
        &self,
        routing_key: ShackInputRoutingKey,
    ) -> Result<ExchangeShackInputPublisher> {
        let publisher = self
            .client
            .new_publisher()
            .await
            .context("unable to set up publisher")?;

        Ok(ExchangeShackInputPublisher {
            publisher,
            routing_key: routing_key.to_routing_key(),
        })
    }
     */
}

impl Stream for ExchangeShackMQTT {
    type Item = Result<(ShackMQTTRoutingKey, Vec<u8>)>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.client.poll_next_unpin(cx).map(|res| {
            res.map(|res| {
                res.and_then(|(key, data)| {
                    let routing_key = match ShackMQTTRoutingKey::from_str(&key) {
                        Ok(k) => k,
                        Err(e) => return Err(e.context("unable to decode routing key").into()),
                    };
                    Ok((routing_key, data))
                })
            })
        })
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
#[non_exhaustive]
pub enum ShackMQTTRoutingKey {
    Tasmota {
        message_type: Option<TasmotaRoutingKey>,
        device_id: Option<String>,
    },
}

impl RoutingKey for ShackMQTTRoutingKey {
    fn to_routing_key(&self) -> String {
        match self {
            ShackMQTTRoutingKey::Tasmota {
                message_type,
                device_id,
            } => {
                format!(
                    "{}.{}.{}",
                    message_type
                        .as_ref()
                        .map(|ty| {
                            match ty {
                                TasmotaRoutingKey::StatResult => ROUTING_PREFIX_TASMOTA_STAT,
                                TasmotaRoutingKey::TeleState => ROUTING_PREFIX_TASMOTA_TELE,
                                TasmotaRoutingKey::TeleSensor => ROUTING_PREFIX_TASMOTA_TELE,
                                TasmotaRoutingKey::TeleLwt => ROUTING_PREFIX_TASMOTA_TELE,
                                TasmotaRoutingKey::TeleInfo1 => ROUTING_PREFIX_TASMOTA_TELE,
                                TasmotaRoutingKey::TeleInfo2 => ROUTING_PREFIX_TASMOTA_TELE,
                                TasmotaRoutingKey::TeleInfo3 => ROUTING_PREFIX_TASMOTA_TELE,
                            }
                        })
                        .unwrap_or_else(|| "*"),
                    device_id
                        .as_ref()
                        .map(|id| format!("tasmota_{}", id))
                        .unwrap_or_else(|| "*".to_string()),
                    message_type
                        .as_ref()
                        .map(|ty| {
                            match ty {
                                TasmotaRoutingKey::StatResult => ROUTING_KEY_TASMOTA_RESULT,
                                TasmotaRoutingKey::TeleState => ROUTING_KEY_TASMOTA_STATE,
                                TasmotaRoutingKey::TeleSensor => ROUTING_KEY_TASMOTA_SENSOR,
                                TasmotaRoutingKey::TeleLwt => ROUTING_KEY_TASMOTA_LWT,
                                TasmotaRoutingKey::TeleInfo1 => ROUTING_KEY_TASMOTA_INFO1,
                                TasmotaRoutingKey::TeleInfo2 => ROUTING_KEY_TASMOTA_INFO2,
                                TasmotaRoutingKey::TeleInfo3 => ROUTING_KEY_TASMOTA_INFO3,
                            }
                        })
                        .unwrap_or_else(|| "*"),
                )
            }
        }
    }

    fn from_str(routing_key: &str) -> Result<Self> {
        let split = routing_key.split('.').collect::<Vec<_>>();
        ensure!(split.len() == 3, "unsupported routing key");

        // Decode the ID
        let device_id = match split.get(1).unwrap().strip_prefix("tasmota_") {
            None => {
                bail!("invalid device ID routing key segment: must start with tasmota_");
            }
            Some(id) => id.to_string(),
        };

        match split[0] {
            ROUTING_PREFIX_TASMOTA_STAT => {
                if *split.get(2).unwrap() != ROUTING_KEY_TASMOTA_RESULT {
                    bail!("unsupported routing key");
                }
                Ok(Self::Tasmota {
                    message_type: Some(TasmotaRoutingKey::StatResult),
                    device_id: Some(device_id),
                })
            }
            ROUTING_PREFIX_TASMOTA_TELE => match *split.get(2).unwrap() {
                ROUTING_KEY_TASMOTA_STATE => Ok(Self::Tasmota {
                    message_type: Some(TasmotaRoutingKey::TeleState),
                    device_id: Some(device_id),
                }),
                ROUTING_KEY_TASMOTA_SENSOR => Ok(Self::Tasmota {
                    message_type: Some(TasmotaRoutingKey::TeleSensor),
                    device_id: Some(device_id),
                }),
                ROUTING_KEY_TASMOTA_LWT => Ok(Self::Tasmota {
                    message_type: Some(TasmotaRoutingKey::TeleLwt),
                    device_id: Some(device_id),
                }),
                ROUTING_KEY_TASMOTA_INFO1 => Ok(Self::Tasmota {
                    message_type: Some(TasmotaRoutingKey::TeleInfo1),
                    device_id: Some(device_id),
                }),
                ROUTING_KEY_TASMOTA_INFO2 => Ok(Self::Tasmota {
                    message_type: Some(TasmotaRoutingKey::TeleInfo2),
                    device_id: Some(device_id),
                }),
                ROUTING_KEY_TASMOTA_INFO3 => Ok(Self::Tasmota {
                    message_type: Some(TasmotaRoutingKey::TeleInfo3),
                    device_id: Some(device_id),
                }),
                _ => bail!("unsupported routing key"),
            },
            _ => {
                bail!("unsupported routing key");
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum TasmotaRoutingKey {
    StatResult,
    TeleState,
    TeleSensor,
    TeleLwt,
    TeleInfo1,
    TeleInfo2,
    TeleInfo3,
}

impl TasmotaRoutingKey {
    pub fn to_str(&self) -> &str {
        match self {
            TasmotaRoutingKey::StatResult => "stat_result",
            TasmotaRoutingKey::TeleState => "tele_state",
            TasmotaRoutingKey::TeleSensor => "tele_sensor",
            TasmotaRoutingKey::TeleLwt => "tele_lwt",
            TasmotaRoutingKey::TeleInfo1 => "tele_info1",
            TasmotaRoutingKey::TeleInfo2 => "tele_info2",
            TasmotaRoutingKey::TeleInfo3 => "tele_info3",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ShackInputRoutingKey {
    pub input_value_type: Option<InputValueType>,
    pub alias: Option<String>,
}

impl ShackInputRoutingKey {
    pub fn from_alias(alias: String) -> ShackInputRoutingKey {
        ShackInputRoutingKey {
            alias: Some(alias),
            input_value_type: None,
        }
    }

    pub fn from_value_type_and_alias(
        value_type: InputValueType,
        alias: String,
    ) -> ShackInputRoutingKey {
        ShackInputRoutingKey {
            alias: Some(alias),
            input_value_type: Some(value_type),
        }
    }
}

impl RoutingKey for ShackInputRoutingKey {
    fn to_routing_key(&self) -> String {
        format!(
            "{}.{}.{}.{}",
            ROUTING_PREFIX_TYPE,
            self.input_value_type
                .as_ref()
                .map(|i| {
                    match i {
                        InputValueType::Binary => ROUTING_KEY_BINARY,
                        InputValueType::Temperature => ROUTING_KEY_TEMPERATURE,
                        InputValueType::Humidity => ROUTING_KEY_HUMIDITY,
                        InputValueType::Pressure => ROUTING_KEY_PRESSURE,
                        InputValueType::Continuous => ROUTING_KEY_CONTINUOUS,
                        InputValueType::Gas => ROUTING_KEY_GAS,
                    }
                })
                .unwrap_or_else(|| "*"),
            ROUTING_PREFIX_ALIAS,
            self.alias
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or_else(|| "*"),
        )
    }

    fn from_str(key: &str) -> Result<Self> {
        let split: Vec<_> = key.split('.').collect();
        if split.len() != 4 {
            bail!("expected 4 parts, found {}", split.len());
        }
        if *split.get(0).unwrap() != ROUTING_PREFIX_TYPE {
            bail!(
                "expected routing key element {} at position 0, found {}",
                ROUTING_PREFIX_TYPE,
                split.get(0).unwrap()
            );
        }
        if *split.get(2).unwrap() != ROUTING_PREFIX_ALIAS {
            bail!(
                "expected routing key element {} at position 2, found {}",
                ROUTING_PREFIX_ALIAS,
                split.get(2).unwrap()
            );
        }

        let input_value_type = match *split.get(1).unwrap() {
            ROUTING_KEY_GAS => InputValueType::Gas,
            ROUTING_KEY_TEMPERATURE => InputValueType::Temperature,
            ROUTING_KEY_BINARY => InputValueType::Binary,
            ROUTING_KEY_PRESSURE => InputValueType::Pressure,
            ROUTING_KEY_HUMIDITY => InputValueType::Humidity,
            ROUTING_KEY_CONTINUOUS => InputValueType::Continuous,
            _ => {
                bail!("unknown input value type {}", split.get(1).unwrap())
            }
        };

        Ok(ShackInputRoutingKey {
            input_value_type: Some(input_value_type),
            alias: Some(split.get(3).unwrap().to_string()),
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
    ShackInput,
    ShackMQTT,
    Logging,
}

impl ExchangeParameters {
    fn name(&self) -> &str {
        match self {
            ExchangeParameters::ShackInput => EXCHANGE_NAME_SHACK_INPUT,
            ExchangeParameters::ShackMQTT => EXCHANGE_NAME_SHACK_MQTT,
            ExchangeParameters::Logging => EXCHANGE_NAME_LOGGING,
        }
    }

    fn message_ttl(&self) -> ShortString {
        match self {
            ExchangeParameters::ShackInput => {
                // 10 seconds
                ShortString::from("10000")
            }
            ExchangeParameters::Logging => {
                // 60 seconds
                ShortString::from("60000")
            }
            ExchangeParameters::ShackMQTT => {
                // 60 seconds
                ShortString::from("60000")
            }
        }
    }

    fn durable(&self) -> bool {
        match self {
            ExchangeParameters::ShackInput => false,
            ExchangeParameters::Logging => true,
            ExchangeParameters::ShackMQTT => false,
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
        ExchangeParameters::ShackInput,
        ExchangeParameters::ShackMQTT,
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
