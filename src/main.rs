mod cron_job;
mod models;

use axum::{
    extract::{Json, Path},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use askama::Template;
use axum::http::StatusCode;
use serde_json::json;
use ngrok::prelude::*;
use ngrok::config::*;
use std::error::Error;
use std::{fs, env};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[pyclass]
struct Detection {
    ts: i32,
    bb: Vec<i32>,
}

#[derive(Serialize, Deserialize, Debug, FromRow)]
struct VideoRecordHTML {
    filepath: String,
    timestamp: String,
    date: String,
    hour: String,
    minute: String,
}

#[derive(Serialize, Deserialize)]
struct ExtractRequest {
    filepath: String,
    detection: Detection,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    videos: &'a [VideoRecordHTML],
}

#[derive(Template)]
#[template(path = "hour.html")]
struct HourTemplate<'a> {
    detections: &'a [Detection],
    filepath: &'a str,
}

async fn index() -> impl IntoResponse {
    let records = models::get_all_records().await;

    let mut filtered_videos = Vec::new();
    for record in records {
        let filepath: String = record.filepath.replace("/", "-");
        let timestamp: String = record.timestamp;
        let detections: Vec<Detection> = serde_json::from_str(&record.detections).unwrap();
        
        let date: String = timestamp.split("T").collect::<Vec<&str>>()[0].to_string();
        let hour: String = timestamp.clone().split("T").collect::<Vec<&str>>()[1].split(":").collect::<Vec<&str>>()[0].to_string();
        let minute: String = timestamp.clone().split("T").collect::<Vec<&str>>()[1].split(":").collect::<Vec<&str>>()[1].to_string();

        if detections.len() > 0 {
            filtered_videos.push(VideoRecordHTML {
                filepath,
                timestamp,
                date,
                hour,
                minute
            });
        }
    }

    let template = IndexTemplate {
        videos: &filtered_videos,
    };

    Html(template.render().unwrap())
}

async fn hour_view(Path(filepath): Path<String>) -> impl IntoResponse {
    let filepath = filepath.replace("-", "/");
    let record = models::get_record_with_filepath(&filepath).await;

    match record {
        Some(record) => {
            let detections: Vec<Detection> = serde_json::from_str(&record.detections).unwrap();
            let template = HourTemplate {
                detections: &detections,
                filepath: &record.filepath,
            };
            Html(template.render().unwrap())
        }
        None => Html("No detections found for this filepath.".to_string()),
    }
}

#[pyfunction]
fn extract_box_as_b64(filepath: &str, detection: Detection) -> PyResult<String> {
    Python::with_gil(|py| {
        let python_code = fs::read_to_string("extract_box.py").unwrap();
        let extract_box_as_b64 = PyModule::from_code_bound(
            py,
            python_code.as_str(),
            "extract_box.py",
            "extract_box",
        )?;

        let image = extract_box_as_b64
            .getattr("extract_box_as_b64")?
            .call1((filepath, detection.ts, detection.bb))?
            .extract()?;
        Ok(image)
    })
}

async fn extract_box(Json(request): Json<ExtractRequest>) -> impl IntoResponse {
    let filepath = request.filepath.clone();
    let detection = request.detection.clone();
    let result = tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| {
            extract_box_as_b64(filepath.as_str(), detection).map_err(|e| e.print_and_set_sys_last_vars(py))
        })
    })
    .await;

    match result {
        Ok(Ok(image)) => (
            StatusCode::OK,
            Json(json!({ "image": image })),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Python error: {:?}", e) })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Blocking error: {:?}", e) })),
        ),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>>{
    let allowed_email = env::var("ALLOWED_EMAIL").expect("ALLOWED_EMAIL must be set");
    let ngrok_domain = env::var("NGROK_DOMAIN").expect("NGROK_DOMAIN must be set");

    // Continuous add new records Cron Job
    tokio::spawn(async {
        cron_job::add_new_records().await;
    });

    // 1hr remove old records Cron Job 
    tokio::spawn(async {
        cron_job::deletion_job::remove_old_records().await;
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/video/:filepath", get(hour_view))
        .route("/extract", post(extract_box));

    let listener = ngrok::Session::builder()
        .authtoken_from_env()
        .connect()
        .await?
        .http_endpoint()
        .oauth(OauthOptions::new("google").allow_email(allowed_email))
        .domain(ngrok_domain)
        .listen()
        .await?;

    axum::Server::builder(listener)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
