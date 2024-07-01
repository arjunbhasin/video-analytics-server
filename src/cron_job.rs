use std::path::{Path, PathBuf};
use std::thread;
use walkdir::WalkDir;
use tokio::time::{self, Duration};
use std::fs;
use pyo3::prelude::*;
use chrono::NaiveDate;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event, EventKind, Config};
use std::sync::mpsc::channel;
use crate::models::*;

const VIDEOS_FOLDER: &str = "/media/baracuda/xiaomi_camera_videos/60DEF4CF9416";

// cron job to add new records to the database
pub async fn add_new_records(){  
    let watch_dir = PathBuf::from(VIDEOS_FOLDER);
    // Create a channel to receive events.
    let (tx, rx) = channel();
    // Automatically select the best implementation for the platform.
    let mut watcher: RecommendedWatcher = Watcher::new(tx, Config::default().with_poll_interval(Duration::from_secs(30))).unwrap();
    
    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(&watch_dir, RecursiveMode::Recursive).unwrap();

    let mut new_filepaths: Vec<String> = Vec::new();

    // get existing file paths from the database
    let db_filepaths = get_filepaths_from_db().await;
    // get all file paths in the videos folder
    let filepaths = get_all_file_paths(VIDEOS_FOLDER);

    // find the file paths that are not in the database and create a stack of them
    for filepath in filepaths {
        if !db_filepaths.contains(&filepath) {
            new_filepaths.push(filepath);
        }
    }

    println!("Unprocessed files count: {:?}", new_filepaths.len());

    // Spawn a new thread to handle new file events.
    thread::spawn(move || {
        loop {
            match rx.recv() {
                Ok(event) => match event {
                    Ok(event) => handle_event(event),
                    Err(e) => println!("watch error: {:?}", e),
                },
                Err(e) => println!("watch error: {:?}", e),
            }
        }
    });

    // Keep the main thread alive.
    loop {
        time::sleep(Duration::from_millis(500)).await;
    }

    // for filepath in new_filepaths {
    //     // get detection string from yolov8
    //     match get_person(&filepath){
    //             Ok(detection_from_yolo) => {
    //                 match extract_datetime_from_path(&filepath) {
    //                     Ok(timestamp) => {

    //                         // create a new record and add it to the database
    //                         let record = DBRecord{
    //                             filepath: filepath.clone(),
    //                             timestamp: timestamp.clone(),
    //                             detections: detection_from_yolo.clone()
    //                         };
                            
    //                         add_record(record).await;
    //                     }
    //                     Err(e) => {
    //                         println!("Failed to extract timestamp from path: {}", e);
    //                     }
    //                 }
    //             }
    //             Err(e) => {
    //                 println!("Failed to get person from yolo: {}", e);
    //             }
    //         }
    //     // sleep for 0.5 second to avoid overloading the system
    //     time::sleep(Duration::from_millis(500)).await;
    // }

}

// cron job to remove old records from the database
pub async fn remove_old_records() {
    let mut interval = time::interval(Duration::from_secs(3600));    
    
    loop {
        interval.tick().await;
        println!("Running deletion job");
        let db_filepaths = get_filepaths_from_db().await;

        for filepath in db_filepaths {
            if !Path::new(filepath.as_str()).exists() {
                delete_record_with_filepath(&filepath).await;
            }
        }
    }
}

/* Helper functions */

fn handle_event(event: Event) {
    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                if path.extension().and_then(|ext| ext.to_str()) == Some("mp4") {
                    println!("New file created: {:?}", path);
                }
            }
        },
        _ => (),
    }
}

fn get_all_file_paths(root: &str) -> Vec<String> {
    let mut file_paths = Vec::new();
    
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if let Some(path_str) = path.to_str() {
                // push only if file is a video file(mp4)
                if path_str.ends_with(".mp4") {
                    file_paths.push(path_str.to_string());
                }
            }
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

    // Extract date from folder name (first 8 characters)
    let date_str = &folder[0..8];
    let date = match NaiveDate::parse_from_str(date_str, "%Y%m%d") {
        Ok(date) => date,
        Err(e) => return Err(format!("Failed to parse date from folder name: {}", e)),
    };

    // Extract hour from folder name (next 2 characters)
    let hour_str = &folder[8..10];
    let hour: u32 = match hour_str.parse() {
        Ok(hour) => hour,
        Err(e) => return Err(format!("Failed to parse hour from folder name: {}", e)),
    };

    // Extract minutes from filename
    let minute_str = filename.split('M').next().unwrap_or("0");
    let minute: u32 = match minute_str.parse() {
        Ok(min) => min,
        Err(_) => 0,
    };

    // Create a NaiveDateTime with the extracted date, hour, minute, and second set to 0
    let dt = date.and_hms_opt(hour, minute, 0).unwrap();

    // Return the ISO formatted datetime string
    Ok(dt.format("%Y-%m-%dT%H:%M:%S").to_string())
}