use std::path::Path;
use tokio::time::{self, Duration};
use crate::models::*;

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
