use crate::db;
use crate::types::SpottedPlate;
use futures::{pin_mut, FutureExt, SinkExt, StreamExt, TryFutureExt};
use log::{debug, error, info, warn};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::{mpsc::Receiver, Mutex};
use tokio::time::delay_for;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

type Callback = Box<dyn FnOnce(bool, Option<&Value>, Arc<Mutex<Connection>>) + Send>;

struct WebSocketWriter {
    sink: futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        Message,
    >,
    next_message_id: AtomicU64,
    callbacks: HashMap<u64, Callback>,
}

impl WebSocketWriter {
    async fn send<F>(&mut self, func: F) -> Result<(), tungstenite::error::Error>
    where
        F: Fn(u64) -> (Value, Option<Callback>),
    {
        let id = self.next_message_id.fetch_add(1, Ordering::Relaxed);
        let (message, callback) = func(id);
        if let Some(callback) = callback {
            self.callbacks.insert(id, callback);
        }
        self.sink.send(Message::text(message.to_string())).await
    }
}

pub async fn run(mut rx: Receiver<SpottedPlate>) {
    let ha_host = env::var("HOST").unwrap_or_else(|_| "localhost:8123".to_string());
    let url = Url::parse(&format!("ws://{}/api/websocket", ha_host)).unwrap();
    loop {
        match tokio_tungstenite::connect_async(&url).await {
            Ok(connection) => handle_connection(connection.0, &mut rx, &ha_host).await,
            Err(e) => error!("Error connecting to websocket: {}", e),
        }
        info!("Waiting 10 seconds and reconnecting to websocket...");
        delay_for(Duration::from_secs(10)).await;
        info!("Reconnecting to websocket...");
    }
}

async fn handle_connection(
    connection: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    rx: &mut Receiver<SpottedPlate>,
    ha_host: &str,
) {
    info!("Handling websocket connection");
    let (ws_write, ws_read) = connection.split();
    let ws_writer = Arc::new(Mutex::new(WebSocketWriter {
        sink: ws_write,
        next_message_id: AtomicU64::new(1),
        callbacks: HashMap::new(),
    }));
    let db_conn = Arc::new(Mutex::new(db::open()));
    let ws_read_future = ws_read
        .for_each(|message_result| async {
            match message_result {
                Ok(message) => {
                    process_websocket_message(message, ws_writer.clone(), db_conn.clone()).await
                }
                Err(e) => error!("Websocket error: {:?}", e),
            }
        })
        .fuse();
    let rx_read_future = rx
        .for_each(|spotted_plate| async {
            if let Err(e) =
                handle_plate(spotted_plate, ws_writer.clone(), ha_host, db_conn.clone()).await
            {
                error!("Error handling spotted plate: {:?}", e);
            }
        })
        .fuse();
    pin_mut!(ws_read_future, rx_read_future);
    futures::select!(
        _ = ws_read_future => info!("Websocket connection dropped"),
        _ = rx_read_future => error!("Rx stream ended"),
    );
}

async fn process_websocket_message(
    message: Message,
    ws_writer: Arc<Mutex<WebSocketWriter>>,
    db_conn: Arc<Mutex<Connection>>,
) {
    // Intentionally log and ignore errors; the stream should remain open
    // unless the server closes it.
    match message {
        Message::Text(s) => {
            if let Err(msg) = handle_websocket_message(&s, ws_writer, db_conn).await {
                error!("Error handling websocket message: {}", msg);
            }
        }
        Message::Close(close_frame) => warn!("Websocket close message: {:?}", close_frame),
        _ => debug!("Ignoring websocket message: {:?}", message),
    }
}

async fn handle_websocket_message(
    s: &str,
    ws_writer: Arc<Mutex<WebSocketWriter>>,
    db_conn: Arc<Mutex<Connection>>,
) -> Result<(), String> {
    let value = serde_json::from_str(s)
        .map_err(|_| format!("Failed to parse websocket message: {:?}", s))
        .and_then(|v| {
            if let Value::Object(o) = v {
                Ok(o)
            } else {
                Err(format!("Unexpected message JSON type: {}", s))
            }
        })?;
    let tp = value["type"]
        .as_str()
        .ok_or_else(|| format!("Unrecognized type for message {}", s))?;
    info!("Handling websocket message of type: {}", tp);
    match tp {
        "auth_required" => {
            let access_token = env::var("ACCESS_TOKEN")
                .map_err(|_| "ACCESS_TOKEN environment variable unset".to_string())?;
            ws_writer
                .lock()
                .await
                .send(|_id| {
                    (
                        json!({ // auth does not use id
                            "type": "auth",
                            "access_token": access_token,
                        }),
                        None, // no id, so no callback
                    )
                })
                .await
                .map_err(|e| format!("Error sending auth message: {}", e))
        }
        "auth_ok" => ws_writer
            .lock()
            .await
            .send(|id| {
                (
                    json!({
                        "id": id,
                        "type": "subscribe_events",
                        "event_type": "mobile_app_notification_action"
                    }),
                    Some(Box::new(|success, data, _| {
                        if success {
                            info!("Successfully subscribed to notification actions");
                        } else {
                            error!("Failed to subscribe to notification actions: {:?}", data);
                        }
                    })),
                )
            })
            .await
            .map_err(|e| format!("Error subscribing to notification actions: {}", e)),
        "result" => handle_result(&value, ws_writer, db_conn).await,
        "event" => handle_event(&value, db_conn).await,
        x => Err(format!("Unrecognized message type {}", x)),
    }
}

async fn handle_event(
    value: &serde_json::Map<String, Value>,
    db_conn: Arc<Mutex<Connection>>,
) -> Result<(), String> {
    let event_type = value["event"]["event_type"].as_str();
    if event_type != Some("mobile_app_notification_action") {
        return Err(format!("Unexpected event type {:?}", event_type));
    }
    let data = &value["event"]["data"];
    let plate = data["action_data"]["plate"]
        .as_str()
        .ok_or_else(|| format!("Missing plate field in data {:?}", data))?;
    let name = data["reply_text"]
        .as_str()
        .ok_or_else(|| format!("Missing text input in data {:?}", data))?;
    info!(
        "Received event requesting name {} for plate {}",
        name, plate
    );
    let conn = &db_conn.lock().await;
    db::update_name(conn, &plate, &name);
    Ok(())
}

async fn handle_result(
    value: &serde_json::Map<String, Value>,
    ws_writer: Arc<Mutex<WebSocketWriter>>,
    db_conn: Arc<Mutex<Connection>>,
) -> Result<(), String> {
    let id = value["id"]
        .as_u64()
        .ok_or_else(|| "No id field in result".to_string())?;
    let success = value["success"]
        .as_bool()
        .ok_or_else(|| "No success field in result".to_string())?;
    if let Some((_, callback)) = ws_writer.lock().await.callbacks.remove_entry(&id) {
        callback(success, value.get("result"), db_conn);
    }
    Ok(())
}

async fn handle_plate(
    spotted_plate: SpottedPlate,
    ws_writer: Arc<Mutex<WebSocketWriter>>,
    ha_host: &str,
    db_conn: Arc<Mutex<Connection>>,
) -> Result<(), String> {
    info!("Spotted plate {:?}", spotted_plate);
    let plate_name = {
        let conn = &db_conn.lock().await;
        db::handle_spotted_plate(conn, &spotted_plate.plate)
    };
    let mut writer = ws_writer.lock().await;
    let mut service_data = json!({
        "message": match &plate_name {
            Some(name) => format!("Spotted plate for {}", name),
            None => format!("Spotted plate {}", &spotted_plate.plate),
        },
        "data": {}
    });
    if plate_name.is_none() {
        // Use an actionable notification to allow specifying the name.
        service_data["data"]["actions"] = json!([
            {
                "action": "REPLY",
                "title": "Save Name...",
                "textInputButtonTitle": "Save",
                "textInputPlaceholder": "e.g. 'John' or 'Trash Pickup'",
            }
        ]);
        service_data["data"]["action_data"] = json!({"plate": spotted_plate.plate});
    }
    if let Some(ref url) = spotted_plate.image_url {
        service_data["data"]["attachment"] = json!({ "url": url });
    }
    writer
        .send(|id| {
            (
                json!({
                    "id": id,
                    "type": "call_service",
                    "domain": "notify",
                    "service": &env::var("NOTIFY_DEVICE").unwrap_or_else(|_| "ALL_DEVICES".to_string()),
                    "service_data": service_data
                }),
                Some(Box::new(|success, data, _| {
                    if success {
                        info!("Successfully sent plate notification");
                    } else {
                        error!("Failed to send plate notification: {:?}", data);
                    }
                })),
            )
        })
        .map_err(|e| format!("Error sending plate notification: {}", e))
        .await?;
    writer
        .send(|id| {
            (
                json!({
                    "id": id,
                    "type": "call_service",
                    "domain": "logbook",
                    "service": "log",
                    "service_data": {
                        "name": format!("License plate {}", spotted_plate.plate),
                        "message": "was spotted",
                        "domain": "camera"
                    }
                }),
                Some(Box::new(|success, _, data| {
                    if success {
                        info!("Successfully wrote to logbook for plate");
                    } else {
                        error!(
                            "Failed to write to logbook for plate notification: {:?}",
                            data
                        );
                    }
                })),
            )
        })
        .map_err(|e| format!("Error writing to logbook for plate event: {}", e))
        .await?;
    let client = reqwest::Client::new();
    let mut map = HashMap::new();
    map.insert("plate", spotted_plate.plate);
    // It would be nice if there were a Home Assistant websocket API call
    // to create an event. Alas, there is not; use the REST API instead.
    client
        .post(&format!(
            "http://{}/api/events/license_plate_spotted",
            ha_host
        ))
        .header(
            "Authorization",
            "Bearer ".to_string()
                + &env::var("ACCESS_TOKEN")
                    .map_err(|_| "ACCESS_TOKEN environment variable unset".to_string())?,
        )
        .json(&map)
        .send()
        .map_err(|e| format!("Error posting plate event: {}", e))
        .await?;
    Ok(())
}
