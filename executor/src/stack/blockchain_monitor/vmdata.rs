use mu_stack::StackID;
use pwr_rs::wallet::PublicKey;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub enum VMData {
    NewStack(NewStack),
    Usage(ServiceUsage),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NewStack {
    pub owner: PublicKey,
    pub revision: u32,
    pub name: String,
    pub stack_data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ServiceUsage {
    pub stack_id: Uuid,
    pub function_mb_instructions: u128, // TODO: should we round a few zeroes off the instruction count?
    pub db_bytes_seconds: u128,
    pub db_reads: u64,
    pub db_writes: u64,
    pub gateway_requests: u64,
    pub gateway_traffic_bytes: u64,
}
