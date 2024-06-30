use std::path::{Path, PathBuf};
use tokio::time::{self, Duration};
use sqlx::{Pool, Sqlite};
use std::{fs, io};
use pyo3::prelude::*;
use chrono::{NaiveDateTime, Datelike, Timelike};
use walkdir::WalkDir;


pub async fn add_new_records(pool: Pool<Sqlite>){
    let db_filepaths = sqlx::query!(
        "SELECT filepath FROM processed_videos"
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    // get all file paths in the videos folder
    let filepaths = get_all_file_paths("/media/baracuda/xiaomi_camera_videos/60DEF4CF9416");
    // find the file paths that are not in the database and create a stack of them
    let mut new_filepaths = Vec::new();
    for filepath in filepaths {
        let filepath_str = filepath.to_str().unwrap();
        if !db_filepaths.iter().any(|db_filepath| db_filepath.filepath.as_deref() == Some(filepath_str)) {
            new_filepaths.push(filepath_str);
        }
    }
    println!("New filepaths: {:?}", new_filepaths);
}

pub async fn remove_old_records(pool: Pool<Sqlite>) {
    let mut interval = time::interval(Duration::from_secs(3600));    
    loop {
        interval.tick().await;
        println!("Running deletion job");
        // delete those records from processed_videos where filepath doesn't exist in videos folder
        let records = sqlx::query!(
            "SELECT filepath FROM processed_videos"
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        for record in records {
            let filepath: String = record.filepath.unwrap_or("".to_string());
            if !Path::new(filepath.as_str()).exists() {
                sqlx::query!(
                    "DELETE FROM processed_videos WHERE filepath = ?",
                    filepath
                )
                .execute(&pool)
                .await
                .unwrap();
                println!("Deleted record with filepath: {}", filepath);
            }
        }
    }
}

fn get_all_file_paths(root: &str) -> Vec<PathBuf> {
    let mut file_paths = Vec::new();
    
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            file_paths.push(path.to_path_buf());
        }
    }
    
    file_paths
}

#[pyfunction]
fn get_person(filepath: &str) -> PyResult<String> {
    Python::with_gil(|py| {
        let python_code = fs::read_to_string("get_person.py").unwrap();
        let get_person_from_filepath = PyModule::from_code_bound(
            py,
            python_code.as_str(),
            "get_person.py",
            "get_person",
        )?;

        let detections: String = get_person_from_filepath
            .getattr("get_person_from_filepath")?
            .call1((filepath,))?
            .extract()?;
        Ok(detections)
    })
}

fn extract_datetime_from_path(filepath: &str) -> Result<String, String> {
    // Convert the filepath to a Path
    let path = Path::new(filepath);

    // Get the parent folder and filename
    let folder = match path.parent() {
        Some(parent) => match parent.file_name() {
            Some(folder_name) => folder_name.to_string_lossy().to_string(),
            None => return Err(format!("Failed to get folder name from path: {}", filepath)),
        },
        None => return Err(format!("Failed to get parent directory from path: {}", filepath)),
    };

    let filename = match path.file_name() {
        Some(file_name) => file_name.to_string_lossy().to_string(),
        None => return Err(format!("Failed to get filename from path: {}", filepath)),
    };

    // Extract date and hour from folder name
    let dt = match NaiveDateTime::parse_from_str(&folder, "%Y%m%d%H") {
        Ok(datetime) => datetime,
        Err(e) => return Err(format!("Failed to parse date and hour from folder name: {}", e)),
    };

    // Extract minutes from filename
    let minute_str = filename.split('M').next().unwrap_or("0");
    let minute: u32 = match minute_str.parse() {
        Ok(min) => min,
        Err(_) => 0,
    };

    // Set minute and second to extracted minute and 0 respectively
    let dt = dt.with_minute(minute).unwrap_or(dt).with_second(0).unwrap_or(dt);

    // Return the ISO formatted datetime string
    Ok(dt.format("%Y-%m-%dT%H:%M:%S").to_string())
}