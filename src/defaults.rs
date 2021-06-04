use {
    dialectic_reconnect::Backoff,
    directories::ProjectDirs,
    std::{
        net::{IpAddr, Ipv6Addr},
        path::PathBuf,
        time::Duration,
    },
};

fn project_dirs() -> Result<ProjectDirs, anyhow::Error> {
    ProjectDirs::from("", shared::ORGANIZATION, shared::APPLICATION)
        .ok_or_else(|| anyhow::anyhow!("Could not open user's home directory"))
}

pub(crate) mod shared {
    pub const ORGANIZATION: &str = "Bolt Labs";

    pub const APPLICATION: &str = "zkchannel";

    pub const fn max_pending_connection_retries() -> usize {
        4
    }

    pub const fn max_message_length() -> usize {
        1024 * 16
    }

    pub const fn port() -> u16 {
        2611
    }
}

pub mod merchant {
    use super::*;

    pub use super::shared::*;

    pub const fn address() -> IpAddr {
        IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0x1))
    }

    pub const CONFIG_FILE: &str = "Merchant.toml";

    pub fn config_path() -> Result<PathBuf, anyhow::Error> {
        Ok(project_dirs()?.config_dir().join(CONFIG_FILE))
    }
}

pub mod customer {
    use super::*;
    use crate::customer::config::DatabaseLocation;

    pub use super::shared::*;

    pub fn backoff() -> Backoff {
        Backoff::with_delay(Duration::from_secs(1))
    }

    pub const fn connection_timeout() -> Option<Duration> {
        Some(Duration::from_secs(60))
    }

    pub const CONFIG_FILE: &str = "Customer.toml";

    pub const DATABASE_FILE: &str = "customer.db";

    pub fn config_path() -> Result<PathBuf, anyhow::Error> {
        Ok(project_dirs()?.config_dir().join(CONFIG_FILE))
    }

    pub fn database_location() -> Result<DatabaseLocation, anyhow::Error> {
        Ok(DatabaseLocation::Sqlite(
            project_dirs()?.data_dir().join(DATABASE_FILE),
        ))
    }

    pub const fn max_note_length() -> u64 {
        1024 * 8
    }
}
