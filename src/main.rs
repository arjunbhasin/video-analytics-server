mod cron_job;
mod models;

use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use askama::Template;
use std::fs;

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
    hour: String
}

#[derive(Serialize, Deserialize)]
struct ExtractRequest {
    filepath: String,
    detection: Detection,
}

async fn index() -> impl Responder {
    let records = models::get_all_records().await;

    let mut filtered_videos = Vec::new();
    for record in records {
        let filepath: String = record.filepath.replace("/", "-");
        let timestamp: String = record.timestamp;
        let detections: Vec<Detection> = serde_json::from_str(&record.detections).unwrap();
        
        let date: String = timestamp.split("T").collect::<Vec<&str>>()[0].to_string();
        let hour: String = timestamp.clone().split("T").collect::<Vec<&str>>()[1].split(":").collect::<Vec<&str>>()[0].to_string();

        if detections.len() > 0 {
            filtered_videos.push(VideoRecordHTML {
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

async fn hour_view(filepath: web::Path<String>) -> impl Responder {
    let filepath = filepath.into_inner().replace("-", "/");
    let record = models::get_record_with_filepath(&filepath).await;

    match record {
        Some(record) => {
            let detections: Vec<Detection> = serde_json::from_str(&record.detections).unwrap();
            let template = HourTemplate {
                detections: &detections,
                filepath: &record.filepath,
            };
            HttpResponse::Ok().body(template.render().unwrap())
        }
        None => HttpResponse::NotFound().body("No detections found for this filepath."),
    }
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {

    // Continuous add new records Cron Job
    actix_rt::spawn(async move {
        cron_job::add_new_records().await;
    });

    // 1hr remove old records Cron Job 
    actix_rt::spawn(async move {
        cron_job::remove_old_records().await;
    });
    
    HttpServer::new(move || {
        App::new()
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