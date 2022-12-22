use borsh::{BorshDeserialize, BorshSerialize};

use crate::gateway;

#[derive(Debug, BorshSerialize)]
pub struct Request<'a>(gateway::Request<'a>);

#[derive(Debug, BorshDeserialize)]
pub struct Response(pub gateway::Response);

impl<'a> Request<'a> {
    pub fn new(request: gateway::Request<'a>) -> Self {
        Self(request)
    }
}
