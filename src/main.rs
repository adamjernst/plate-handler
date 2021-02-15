mod db;
mod types;
mod webhook;
mod websocket;

use env_logger::Env;
use log::{error, info};
use tokio::sync::mpsc::channel;

#[macro_use]
extern crate failure;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("Starting plate-handler");

    let (tx, rx) = channel(8);
    let websocket_task = tokio::spawn(websocket::run(rx));
    let webhook_task = tokio::spawn(webhook::run(tx));
    tokio::select! {
        result = websocket_task => {
            if let Err(e) = result {
                error!("Websocket task failed: {}", e);
            }
        }
        result = webhook_task => {
            if let Err(e) = result {
                error!("Webhook task failed: {}", e);
            }
        }
    }
    info!("Exiting main");
}
