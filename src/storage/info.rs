use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StorageInfo {
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub host_id: String,  // Host identifier (e.g., "10.82.128.41:9090")
    pub hostname: String, // DNS hostname of the server
    pub index: u32,
}
