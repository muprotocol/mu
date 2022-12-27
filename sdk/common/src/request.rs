use std::borrow::Cow;

use borsh::BorshDeserialize;

#[derive(BorshDeserialize)]
pub struct Request<'a> {
    pub path: Cow<'a, str>,
    pub body: Cow<'a, [u8]>,
}
