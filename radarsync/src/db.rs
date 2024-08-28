use anyhow::{bail, Context};
use doppler_ws::model::Device;
use sqlx::SqlitePool;

pub struct Library {
    db: sqlx::sqlite::SqlitePool,
}

impl Library {
    /// Opens a connection to the library database.
    pub async fn open() -> anyhow::Result<Self> {
        let Some(mut data_dir) = dirs::data_dir() else {
            bail!("Couldn't figure out where to put the library database");
        };
        data_dir.push("radarsync");

        if !data_dir.exists() {
            tracing::debug!("Creating config dir {}", data_dir.display());
            std::fs::create_dir(&data_dir)
                .with_context(|| format!("Error creating {}", data_dir.display()))?;
        }

        let db = {
            let db_path = data_dir.join("library.db");
            let Some(db_path_str) = db_path.to_str() else {
                bail!("Data directory path is not UTF-8, can't create library");
            };
            let db_url = format!("sqlite://{db_path_str}?mode=rwc");
            tracing::debug!("Opening database {db_url}");

            SqlitePool::connect(&db_url).await?
        };

        sqlx::migrate!("db/migrations").run(&db).await?;

        Ok(Self { db })
    }

    /// Gets a saved device with the provided name.
    pub async fn get_device(&self, name: impl AsRef<str>) -> anyhow::Result<Option<Device>> {
        let name = name.as_ref();
        let mut conn = self.db.acquire().await?;
        let response = match sqlx::query!("SELECT data FROM devices WHERE name = ?", name)
            .fetch_one(conn.as_mut())
            .await
        {
            Ok(res) => res,
            Err(sqlx::Error::RowNotFound) => {
                return Ok(None);
            }
            Err(err) => {
                return Err(err.into());
            }
        };
        let device: Device = serde_json::from_str(&response.data)?;
        Ok(Some(device))
    }

    /// Gets a Device from the database by its ID, if it exists.
    pub async fn get_device_by_id(&self, id: impl AsRef<str>) -> anyhow::Result<Option<Device>> {
        let id = id.as_ref();
        let mut conn = self.db.acquire().await?;
        let response = match sqlx::query!("SELECT data FROM devices WHERE id = ?", id)
            .fetch_one(conn.as_mut())
            .await
        {
            Ok(res) => res,
            Err(sqlx::Error::RowNotFound) => {
                return Ok(None);
            }
            Err(err) => {
                return Err(err.into());
            }
        };
        let device: Device = serde_json::from_str(&response.data)?;
        Ok(Some(device))
    }

    /// Saves the device to the library database.
    pub async fn add_device(&self, device: &Device) -> anyhow::Result<()> {
        let Some(device_name) = &device.name else {
            bail!("Missing device name");
        };
        let Some(device_id) = &device.id else {
            bail!("Missing device ID");
        };
        let mut conn = self.db.acquire().await?;
        let device_str = serde_json::to_string(device)?;
        sqlx::query!(
            "INSERT INTO devices (id, name, data) VALUES (?, ?, ?)",
            device_id,
            device_name,
            device_str,
        )
        .execute(conn.as_mut())
        .await?;
        Ok(())
    }
}
