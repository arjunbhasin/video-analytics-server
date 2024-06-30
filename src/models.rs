use std::env;
use sqlx::SqlitePool;

pub async fn get_filepaths_from_db() -> Vec<String> {
    let database_url: String = env::var("DATABASE_URL").unwrap_or("sqlite:///root/workspace/processing_results.db".to_string());
    let db = SqlitePool::connect(&database_url).await.unwrap();

    let db_filepaths = sqlx::query!(
        "SELECT filepath FROM processed_videos"
    )
    .fetch_all(&db)
    .await
    .unwrap();
    
    // close the db connection
    db.close().await;

    db_filepaths.iter().map(|db_filepath| db_filepath.filepath.as_deref().unwrap().to_string()).collect()
}

pub async fn add_record(filepath: &str, timestamp: &str, detections: &str) {
    let database_url: String = env::var("DATABASE_URL").unwrap_or("sqlite:///root/workspace/processing_results.db".to_string());
    let db = SqlitePool::connect(&database_url).await.unwrap();

    println!("Adding record with filepath: {}", filepath);
    
    sqlx::query!(
        "INSERT INTO processed_videos (filepath, timestamp, detections) VALUES (?, ?, ?)",
        filepath,
        timestamp,
        detections
    )
    .execute(&db)
    .await
    .unwrap();
    
    println!("Added record with filepath: {}", filepath);

    // close the db connection
    db.close().await;
}

pub async fn delete_record_with_filepath(filepath: &str) {
    let database_url: String = env::var("DATABASE_URL").unwrap_or("sqlite:///root/workspace/processing_results.db".to_string());
    let db = SqlitePool::connect(&database_url).await.unwrap();

    sqlx::query!(
        "DELETE FROM processed_videos WHERE filepath = ?",
        filepath
    )
    .execute(&db)
    .await
    .unwrap();
    
    println!("Deleted record with filepath: {}", filepath);

    // close the db connection
    db.close().await;
}