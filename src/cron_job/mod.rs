pub mod deletion_job;
mod yolo;

use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;
use tokio::time::{self, Duration};

use chrono::NaiveDate;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event, EventKind, Config};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::sync::Notify;
use crate::models::*;
use yolo::get_person;

const VIDEOS_FOLDER: &str = "/media/baracuda/xiaomi_camera_videos/60DEF4CF9416";

// cron job to add new records to the database
pub async fn add_new_records(){  
    let watch_dir = PathBuf::from(VIDEOS_FOLDER);
    let new_filepaths: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let notify = Arc::new(Notify::new());

    // Get existing file paths from the database
    let db_filepaths = get_filepaths_from_db().await;
    // Get all file paths in the videos folder
    let filepaths = get_all_file_paths(VIDEOS_FOLDER);

    // Find the file paths that are not in the database and create a stack of them
    {
        let mut new_filepaths_lock = new_filepaths.lock().unwrap();
        for filepath in filepaths {
            if !db_filepaths.contains(&filepath) {
                new_filepaths_lock.push(filepath);
            }
        }
    }

    // Notify that initial filepaths are ready to be processed
    notify.notify_one();

    // Create a channel to receive events.
    let (tx, mut rx) = mpsc::channel(100);

    // Automatically select the best implementation for the platform.
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                tx.blocking_send(event).unwrap();
            }
        },
        Config::default().with_poll_interval(Duration::from_secs(30)),
    )
    .unwrap();

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(&watch_dir, RecursiveMode::Recursive).unwrap();

   // Clone the Arc for the new thread
   let new_filepaths_clone = Arc::clone(&new_filepaths);
   let notify_clone = Arc::clone(&notify);

    // Spawn a new task to handle new file events.
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            handle_event(event, Arc::clone(&new_filepaths_clone), Arc::clone(&notify_clone));
        }
    });

    loop {
        while let Some(filepath) = {
            let mut new_filepaths = new_filepaths.lock().unwrap();
            if !new_filepaths.is_empty() {
                Some(new_filepaths.pop().unwrap())
            } else {
                None
            }
        } {
            println!("Unprocessed file paths: {:?}", new_filepaths.lock().unwrap().len());
            match process_filepath(&filepath).await {
                Ok(_) => {
                    println!("File processed successfully: {}", filepath);
                }
                Err(_) => {
                    println!("Failed to process file: {}", filepath);
                    // Add the filepath back to the stack if it failed to process
                    let mut new_filepaths = new_filepaths.lock().unwrap();
                    new_filepaths.push(filepath);
                }
            }
            // Sleep for a short duration to avoid processing files too quickly
            time::sleep(Duration::from_millis(500)).await;
        }
        
        // Sleep for a short duration if the stack is empty
        time::sleep(Duration::from_secs(30)).await;
    }
}


fn handle_event(
    event: Event,
    new_filepaths: Arc<Mutex<Vec<String>>>,
    notify: Arc<Notify>,
) {
    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                // file should end with .mp4 and not start with a dot
                if path.is_file() && path.extension().map_or(false, |e| e == "mp4") && !path.file_name().map_or(false, |f| f.to_str().map_or(false, |f| f.starts_with('.'))) {
                    let mut new_filepaths = new_filepaths.lock().unwrap();
                    new_filepaths.push(path.to_string_lossy().to_string());
                    println!("New file created and added to vector: {:?}", path);
                    notify.notify_one();
                }
            }
        }
        _ => (),
    }
}

// Check if a file is stable by checking its size at regular intervals
fn is_file_stable(filepath: &str, duration: Duration, checks: u32) -> bool {
    let mut previous_size = None;
    for _ in 0..checks {
        let metadata = fs::metadata(filepath);
        let current_size = metadata.map(|m| m.len()).ok();
        if current_size == previous_size {
            return true;
        }
        previous_size = current_size;
        std::thread::sleep(duration);
    }
    false
}

async fn process_filepath(filepath: &str) -> Result<(),()> {
    if !is_file_stable(filepath, Duration::from_secs(2), 3) {
        println!("File is not stable yet: {}", filepath);
        return Err(());
    }

    match get_person(&filepath){
        Ok(detection_from_yolo) => {
            match extract_datetime_from_path(&filepath) {
                Ok(timestamp) => {

                    // create a new record and add it to the database
                    let record = DBRecord{
                        filepath: filepath.to_string(),
                        timestamp: timestamp.clone(),
                        detections: detection_from_yolo.clone()
                    };
                    
                    add_record(record).await;
                    Ok(())
                }
                Err(e) => {
                    println!("Failed to extract timestamp from path: {}", e);
                    Err(())
                }
            }
        }
        Err(e) => {
            println!("Failed to get person from yolo: {}", e);
            Err(())
        }
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