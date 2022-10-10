use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use dyn_clonable::clonable;
use futures::{future::BoxFuture, stream::BoxStream};
use mailbox_processor::{plain::PlainMailboxProcessor, ReplyChannel};
use mu_stack::StackID;
use solana_client::{
    nonblocking::pubsub_client::PubsubClient,
    rpc_response::{RpcKeyedAccount, RpcResult},
};

use super::{StackMetadata, StackWithMetadata};

#[async_trait]
#[clonable]
pub trait BlockchainMonitor: Clone + Send + Sync {
    async fn get_stack(&self, stack_id: StackID) -> Result<Option<StackWithMetadata>>;
    async fn get_metadata(&self, stack_id: StackID) -> Result<Option<StackMetadata>>;
    async fn stop(&self) -> Result<()>;
}

pub enum BlockchainMonitorNotifications {
    // TODO: monitor for removed/undeployed stacks
    StacksAvailable(Vec<StackWithMetadata>),
}

type SolanaUnsubscribeFn = Box<dyn FnOnce() -> BoxFuture<'static, ()> + Send>;

struct State<'a> {
    known_stacks: HashMap<StackID, StackWithMetadata>,

    solana_pub_sub_client: PubsubClient,
    solana_pub_sub_stream: BoxStream<'a, RpcResult<RpcKeyedAccount>>,
    solana_unsub_callback: SolanaUnsubscribeFn,
}

enum BlockchainMonitorMessage {
    GetStack(StackID, ReplyChannel<Option<StackWithMetadata>>),
    GetMetadata(StackID, ReplyChannel<Option<StackMetadata>>),
    Stop(ReplyChannel<()>),
}

#[derive(Clone)]
struct BlockchainMonitorImpl {
    mailbox: PlainMailboxProcessor<BlockchainMonitorMessage>,
}

#[async_trait]
impl BlockchainMonitor for BlockchainMonitorImpl {
    async fn get_stack(&self, stack_id: StackID) -> Result<Option<StackWithMetadata>> {
        self.mailbox
            .post_and_reply(|r| BlockchainMonitorMessage::GetStack(stack_id, r))
            .await
            .map_err(Into::into)
    }

    async fn get_metadata(&self, stack_id: StackID) -> Result<Option<StackMetadata>> {
        self.mailbox
            .post_and_reply(|r| BlockchainMonitorMessage::GetMetadata(stack_id, r))
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) -> Result<()> {
        self.mailbox
            .post_and_reply(|r| BlockchainMonitorMessage::Stop(r))
            .await
            .map_err(Into::into)
    }
}
