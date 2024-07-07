use std::env;
use sqlx::{SqlitePool, FromRow};

#[derive(Debug, FromRow, Clone)]
pub struct DBRecord {
    pub filepath: String,
    pub timestamp: String,
    pub detections: String,
}

fn get_db_url() -> String {
    env::var("DATABASE_URL").unwrap_or("sqlite:///root/workspace/processing_results.db".to_string())
}

pub async fn get_all_records() -> Vec<DBRecord> {
    let database_url: String = get_db_url();
    let db = SqlitePool::connect(&database_url).await.unwrap();

    // get all records from the database sorted by timestamp(latest first)
    let db_records = sqlx::query_as::<_,DBRecord>(
        "SELECT * FROM processed_videos ORDER BY timestamp DESC"
    )
    .fetch_all(&db)
    .await
    .unwrap();
    
    // // close the db connection
    // db.close().await;

    db_records
}

// pub async fn get_non_empty_records() -> Vec<DBRecord> {
//     let database_url: String = get_db_url();
//     let db = SqlitePool::connect(&database_url).await.unwrap();

//     let db_records = sqlx::query_as::<_,DBRecord>(
//         "SELECT * FROM processed_videos WHERE detections IS NOT []"
//     )
//     .fetch_all(&db)
//     .await
//     .unwrap();
    
//     // close the db connection
//     db.close().await;

//     db_records
// }

pub async fn get_filepaths_from_db() -> Vec<String> {
    let database_url: String = get_db_url();
    let connection_result = SqlitePool::connect(&database_url).await;
    
    let db = match connection_result {
        Ok(db) => db,
        Err(e) => {
            println!("Failed to connect to the database: {}", e);
            return Vec::new();
        }
    };

    #[derive(Debug, FromRow)]
    struct DBFilepath {
        pub filepath: String,
    }

    let db_filepaths = sqlx::query_as::<_, DBFilepath>(
        "SELECT filepath FROM processed_videos"
    )
    .fetch_all(&db)
    .await
    .unwrap();
    
    // close the db connection
    // db.close().await;

    db_filepaths.iter().map(|x| x.filepath.clone()).collect()
}

pub async fn get_record_with_filepath(filepath: &str) -> Option<DBRecord> {
    let database_url: String = get_db_url();
    let db = SqlitePool::connect(&database_url).await.unwrap();

    let record = sqlx::query_as::<_,DBRecord>(
        "SELECT * FROM processed_videos WHERE filepath = ?"
    )
    .bind(filepath)
    .fetch_one(&db)
    .await
    ;

    match record {
        Ok(record) => {
            Some(record)
        },
        Err(_) => {
            None
        }
    }
}
pub async fn add_record(record: DBRecord) {
    let database_url: String = get_db_url();
    let db = SqlitePool::connect(&database_url).await.unwrap();
    
    // insert the record into the database
    let insertion_result = sqlx::query(
        "INSERT INTO processed_videos (filepath, timestamp, detections) VALUES (?, ?, ?)"
    )
    .bind(&record.filepath)
    .bind(&record.timestamp)
    .bind(&record.detections)
    .execute(&db)
    .await
    ;

    match insertion_result {
        Ok(_) => {
            // println!("Inserted record with filepath: {}", record.filepath);
        },
        Err(e) => {
            println!("Failed to insert record: {}", e);
        }
        
    }
    // close the db connection
    // db.close().await;
}

pub async fn delete_record_with_filepath(filepath: &str) {
    let database_url: String = get_db_url();
    let db = SqlitePool::connect(&database_url).await.unwrap();

    let deletion_result = sqlx::query(
        "DELETE FROM processed_videos WHERE filepath = ?"
    )
    .bind(filepath)
    .execute(&db)
    .await
    ;

    match deletion_result {
        Ok(_) => {
            println!("Deleted record with filepath: {}", filepath);
        },
        Err(e) => {
            println!("Failed to delete record: {}", e);
        }
    }
    // close the db connection
    // db.close().await;
}