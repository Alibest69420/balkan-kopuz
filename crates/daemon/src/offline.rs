//! An in-memory mirror of the `offline_tracks` table, so wire rows and playback never wait on the database.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use api::ApiError;

#[derive(Clone, Default)]
pub struct OfflineIndex {
    paths: Arc<RwLock<HashMap<String, String>>>,
}

impl OfflineIndex {
    pub async fn load(db: &db::Db) -> Result<Self, db::DbError> {
        let paths = db.offline_tracks().await?;
        Ok(Self {
            paths: Arc::new(RwLock::new(paths)),
        })
    }

    /// The registered copy of `item_id`, whether or not the file is still there.
    pub fn path(&self, item_id: &str) -> Option<String> {
        self.read().get(item_id).cloned()
    }

    pub fn ids(&self) -> Vec<String> {
        self.read().keys().cloned().collect()
    }

    /// Register (`Some`) or forget (`None`) a copy; the map follows only once the row is written.
    pub async fn set(
        &self,
        db: &db::Db,
        item_id: &str,
        path: Option<String>,
    ) -> Result<(), ApiError> {
        db.set_offline_track(item_id, path.as_deref())
            .await
            .map_err(|error| {
                ApiError::internal(format!("offline track registration failed: {error}"))
            })?;
        let mut paths = self
            .paths
            .write()
            .unwrap_or_else(|poison| poison.into_inner());
        match path {
            Some(path) => paths.insert(item_id.to_string(), path),
            None => paths.remove(item_id),
        };
        Ok(())
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, HashMap<String, String>> {
        self.paths
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}
