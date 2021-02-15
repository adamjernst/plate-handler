use crate::types::SpottedPlate;
use bytes::Buf;
use futures::StreamExt;
use image::imageops::FilterType;
use image::io::Reader as ImageReader;
use image::ImageFormat;
use log::{debug, error, info, warn};
use serde_json::Value;
use std::env;
use std::io::Cursor;
use std::path::Path;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;
use warp::Filter;

#[derive(Debug)]
struct HandlePlateError;

impl warp::reject::Reject for HandlePlateError {}

pub async fn run(tx: Sender<SpottedPlate>) {
    let routes = warp::post()
        .and(warp::path!("webhook"))
        .and(warp::filters::multipart::form())
        .and(warp::any().map(move || tx.clone()))
        .and_then(|form, tx| async {
            let result = handle_plate(form, tx).await;
            if let Err(e) = &result {
                error!("Error handling plate: {:?}", e);
            }
            result.map_err(|_| warp::reject::custom(HandlePlateError))
        });
    warp::serve(routes).run(([0, 0, 0, 0], 8402)).await;
}

async fn handle_plate(
    mut form: warp::filters::multipart::FormData,
    mut tx: Sender<SpottedPlate>,
) -> Result<impl warp::Reply, Box<dyn std::error::Error>> {
    let mut json: Option<Value> = None;
    let mut image: Option<image::DynamicImage> = None;

    while let Some(part) = form.next().await {
        let part = part?;
        debug!("Got part {}", part.name());
        match part.name() {
            "json" => {
                // Well this is stupid. Serde doesn't integrate with Tokio yet,
                // so we have to collect all the data and then pass it to Serde.
                // There is probably a more elegant way to do this.
                let mut data: Vec<u8> = vec![];
                let mut stream = part.stream();
                while let Some(buf) = stream.next().await {
                    data.extend_from_slice(buf?.bytes());
                }
                json = Some(serde_json::from_slice::<Value>(&data)?);
            }
            "upload" => {
                // Image crate doesn't integrate with Tokio yet either...
                let mut data: Vec<u8> = vec![];
                let mut stream = part.stream();
                while let Some(buf) = stream.next().await {
                    data.extend_from_slice(buf?.bytes());
                }
                // Explicitly ignore decode errors, mapping them to None.
                // The image is not a required part of the notification.
                image =
                    match ImageReader::with_format(Cursor::new(data), ImageFormat::Jpeg).decode() {
                        Ok(i) => Some(i),
                        Err(e) => {
                            warn!("Failed to decode image: {:?}", e);
                            None
                        }
                    }
            }
            _ => {
                warn!("Ignoring part {}", part.name());
            }
        }
    }

    let json = json.ok_or_else(|| format_err!("Missing JSON data"))?;
    let results = json["data"]["results"]
        .as_array()
        .ok_or_else(|| format_err!("Missing results in JSON"))?;

    let image_url = match (image, env::var("PLATES_URL")) {
        (Some(i), Ok(plates_url)) => {
            let name = format!("{:x}.jpeg", Uuid::new_v4().to_simple());
            let path = Path::new("/plates").join(&name);
            match i.resize(1024, 768, FilterType::Triangle).save(&path) {
                Ok(_) => Some(plates_url + &name),
                Err(e) => {
                    warn!("Error saving image to {:?}: {:?}", path, e);
                    None
                }
            }
        }
        _ => None,
    };

    for result in results {
        let plate = result["plate"]
            .as_str()
            .ok_or_else(|| format_err!("Missing plate field"))?;
        info!("Sending plate {} to tx", plate);
        tx.send(SpottedPlate {
            plate: plate.to_string().to_ascii_uppercase(),
            score: result["score"]
                .as_f64()
                .ok_or_else(|| format_err!("Missing score field"))?,
            vehicle_type: result["vehicle"]["type"]
                .as_str()
                .ok_or_else(|| format_err!("Missing vehicle type field"))?
                .to_string(),
            image_url: image_url.clone(),
        })
        .await?;
    }

    Ok(warp::reply())
}
