use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::env;
use askama::Template;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[pyclass]
struct Detection {
    ts: i32,
    bb: Vec<i32>,
}

#[derive(Serialize, Deserialize, Debug)]
struct VideoRecord {
    filepath: String,
    timestamp: String,
    date: String,
    hour: String
}

#[derive(Serialize, Deserialize)]
struct ExtractRequest {
    filepath: String,
    detection: Detection,
}

async fn index(pool: web::Data<SqlitePool>) -> impl Responder {
    let records = sqlx::query!(
        "SELECT filepath, timestamp, detections FROM processed_videos"
    )
    .fetch_all(pool.get_ref())
    .await
    .unwrap();

    let mut filtered_videos = Vec::new();
    for record in records {
        let filepath: String = record.filepath.unwrap_or("".to_string()).replace("/", "-");
        let timestamp: String = record.timestamp.unwrap_or("".to_string());
        let detections: Vec<Detection> = serde_json::from_str(&record.detections.unwrap()).unwrap();
        
        let date: String = timestamp.split("T").collect::<Vec<&str>>()[0].to_string();
        let hour: String = timestamp.clone().split("T").collect::<Vec<&str>>()[1].split(":").collect::<Vec<&str>>()[0].to_string();

        if detections.len() > 0 {
            filtered_videos.push(VideoRecord {
                filepath,
                timestamp,
                date,
                hour
            });
        }
    }

    let template = IndexTemplate {
        videos: &filtered_videos,
    };

    HttpResponse::Ok().body(template.render().unwrap())
}

async fn hour_view(filepath: web::Path<String>, pool: web::Data<SqlitePool>) -> impl Responder {
    let filepath = filepath.into_inner().replace("-", "/");
    let record = sqlx::query!(
        "SELECT filepath, timestamp, detections FROM processed_videos WHERE filepath = ?",
        filepath
    )
    .fetch_one(pool.get_ref())
    .await;

    match record {
        Ok(record) => {
            let detections: Vec<Detection> = serde_json::from_str(&record.detections.unwrap()).unwrap();
            let template = HourTemplate {
                detections: &detections,
                filepath: &record.filepath.unwrap(),
            };
            HttpResponse::Ok().body(template.render().unwrap())
        }
        Err(_) => HttpResponse::NotFound().body("No detections found for this video."),
    }
}


#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    videos: &'a [VideoRecord],
}

#[derive(Template)]
#[template(path = "hour.html")]
struct HourTemplate<'a> {
    detections: &'a [Detection],
    filepath: &'a str,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let database_url = env::var("DATABASE_URL").unwrap_or("../processing_results.db".to_string());

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .unwrap();

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .route("/", web::get().to(index))
            .route("/video/{filepath:.*}", web::get().to(hour_view))
            .route("/extract", web::post().to(extract_box))
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}

#[pyfunction]
fn extract_box_as_b64(filepath: &str, detection: Detection) -> PyResult<String> {
    Python::with_gil(|py| {
        let extract_box_as_b64 = PyModule::from_code_bound(
            py,
            PYTHON_CODE,
            "extract_box.py",
            "extract_box",
        )?;

        let image = extract_box_as_b64
            .getattr("extract_box_as_b64")?
            .call1((filepath, detection))?
            .extract()?;
        Ok(image)
    })
}

async fn extract_box(request: web::Json<ExtractRequest>) -> impl Responder {
    let filepath = request.filepath.clone();
    let detection = request.detection.clone();
    let result = web::block(move || {
        Python::with_gil(|py| {
            extract_box_as_b64(filepath.as_str(), detection).map_err(|e| e.print_and_set_sys_last_vars(py))
        })
    })
    .await;

    match result {
        Ok(Ok(image)) => HttpResponse::Ok().json(serde_json::json!({ "image": image })),
        Ok(Err(e)) => HttpResponse::InternalServerError().body(format!("Python error: {:?}", e)),
        Err(e) => HttpResponse::InternalServerError().body(format!("Blocking error: {:?}", e)),
    }
}

const PYTHON_CODE: &str = r#"
import cv2
from PIL import Image
import io
import base64
from typing import List, Dict

def extract_box_as_b64(filepath: str, detection: Dict[str, List[int]]) -> str:
    cap = cv2.VideoCapture(filepath)

    if not cap.isOpened():
        raise FileNotFoundError(f"Cannot open video file: {filepath}")

    fps = int(cap.get(cv2.CAP_PROP_FPS))

    ts = detection['ts']
    bb = detection['bb']

    # Calculate the frame number
    frame_number = ts * fps
    cap.set(cv2.CAP_PROP_POS_FRAMES, frame_number)

    ret, frame = cap.read()
    if not ret:
        cap.release()
        raise ValueError(f"Failed to read frame at timestamp {ts} seconds")

    # Extract bounding box coordinates
    x1, y1, x2, y2 = bb

    # Crop the bounding box from the frame
    cropped_img = frame[y1:y2, x1:x2]

    # Convert the cropped image to a PIL Image
    cropped_img_pil = Image.fromarray(cv2.cvtColor(cropped_img, cv2.COLOR_BGR2RGB))

    # Convert PIL Image to base64
    with io.BytesIO() as output:
        cropped_img_pil.save(output, format="PNG")
        b64_data = base64.b64encode(output.getvalue()).decode()

    cap.release()
    return b64_data
"#;