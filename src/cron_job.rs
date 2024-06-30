// use std::env;
use std::path::Path;
use tokio::time::{self, Duration};
use sqlx::{Pool, Sqlite};

pub async fn add_new_records(pool: Pool<Sqlite>){
    
}

pub async fn remove_old_records(pool: Pool<Sqlite>) {
    // let database_url: String = env::var("DATABASE_URL").unwrap_or("sqlite:///root/workspace/processing_results.db".to_string());
    let mut interval = time::interval(Duration::from_secs(3600));
    
    // let pool = SqlitePoolOptions::new()
    // .max_connections(1)
    // .connect(&database_url)
    // .await
    // .unwrap();

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