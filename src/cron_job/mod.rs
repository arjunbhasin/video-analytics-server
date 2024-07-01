pub mod deletion_job;
mod yolo;

use std::path::{Path, PathBuf};
use std::thread;
use walkdir::WalkDir;
use tokio::time::{self, Duration};

use chrono::NaiveDate;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event, EventKind, Config};
use std::sync::{Arc, Mutex, mpsc::channel};
use crate::models::*;
use yolo::get_person;

const VIDEOS_FOLDER: &str = "/media/baracuda/xiaomi_camera_videos/60DEF4CF9416";

// cron job to add new records to the database
pub async fn add_new_records(){  
    let watch_dir = PathBuf::from(VIDEOS_FOLDER);
    let new_filepaths: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let notify = Arc::new(tokio::sync::Notify::new());

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
    let (tx, rx) = channel();

    // Automatically select the best implementation for the platform.
    let mut watcher: RecommendedWatcher = Watcher::new(tx, Config::default().with_poll_interval(Duration::from_secs(30))).unwrap();
    
    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(&watch_dir, RecursiveMode::Recursive).unwrap();

   // Clone the Arc for the new thread
   let new_filepaths_clone = Arc::clone(&new_filepaths);
   let notify_clone = Arc::clone(&notify);

    // Spawn a new thread to handle new file events.
    thread::spawn(move || {
        loop {
            match rx.recv() {
                Ok(event) => match event {
                    Ok(event) => handle_event(event, Arc::clone(&new_filepaths_clone), Arc::clone(&notify_clone)),
                    Err(e) => println!("watch error: {:?}", e),
                },
                Err(e) => println!("watch error: {:?}", e),
            }
        }
    });

    loop {
        // Wait for notification that there are new file paths to process
        notify.notified().await;

        while let Some(filepath) = {
            let mut new_filepaths = new_filepaths.lock().unwrap();
            if !new_filepaths.is_empty() {
                Some(new_filepaths.pop().unwrap())
            } else {
                None
            }
        } {
            println!("Unprocessed file paths: {:?}", new_filepaths.lock().unwrap().len());
            println!("Processing file: {}", filepath);
            process_filepath(&filepath).await;
            time::sleep(Duration::from_millis(500)).await;
        }
        
        // Sleep for a short duration if the stack is empty
        time::sleep(Duration::from_secs(30)).await;
    }
}


fn handle_event(event: Event, new_filepaths: Arc<Mutex<Vec<String>>>, notify: Arc<tokio::sync::Notify>) {
    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                if path.extension().and_then(|ext| ext.to_str()) == Some("mp4") {
                    let mut new_filepaths = new_filepaths.lock().unwrap();
                    new_filepaths.push(path.to_string_lossy().to_string());
                    println!("New file created and added to vector: {:?}", path);
                    notify.notify_one();
                }
            }
        },
        _ => (),
    }
}

async fn process_filepath(filepath: &str) {
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
                }
                Err(e) => {
                    println!("Failed to extract timestamp from path: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Failed to get person from yolo: {}", e);
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