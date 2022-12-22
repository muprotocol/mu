use borsh::BorshDeserialize;

#[derive(Debug, BorshDeserialize)]
pub struct Log {
    pub body: String, //TODO: use &str if can
}
