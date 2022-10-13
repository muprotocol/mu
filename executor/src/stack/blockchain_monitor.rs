use std::{collections::HashMap, marker::PhantomPinned, ops::Deref, pin::Pin};

use anchor_client::anchor_lang::AnchorDeserialize;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use futures::{future::BoxFuture, stream::BoxStream, StreamExt};
use log::{info, warn};
use mailbox_processor::{
    plain::{MessageReceiver, PlainMailboxProcessor},
    NotificationChannel, ReplyChannel,
};
use mu_stack::StackID;
use serde::Deserialize;
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient},
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, MemcmpEncoding, RpcFilterType},
    rpc_response::{Response, RpcKeyedAccount},
};
use solana_sdk::{account::Account, commitment_config::CommitmentConfig, pubkey::Pubkey};
use tokio::{select, sync::mpsc::UnboundedReceiver};

use super::{config_types::Base58PublicKey, StackMetadata, StackWithMetadata};

//TODO: usage updates and escrow
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

#[derive(Deserialize)]
pub struct BlockchainMonitorConfig {
    solana_cluster_rpc_url: String,
    solana_cluster_pub_sub_url: String,
    solana_provider_public_key: Base58PublicKey,
    solana_region_number: u32,
    // solana_usage_signer_private_key: Base58PrivateKey,
    // solana_min_escrow_balance: u64,
}

type SolanaUnsubscribeFn = Box<dyn FnOnce() -> BoxFuture<'static, ()> + Send>;

// Since subscription streams hold a reference to the `PubsubClient`, we
// need to keep the client in a fixed memory location, so we pin it using
// this struct.
struct SolanaPubSubClientWrapper {
    client: PubsubClient,
    _phantom_pinned: PhantomPinned,
}

struct SolanaPubSub<'a> {
    client_wrapper: Pin<Box<SolanaPubSubClientWrapper>>,
    stream: BoxStream<'a, Response<RpcKeyedAccount>>,
    unsub_callback: SolanaUnsubscribeFn,
}

struct BlockchainMonitorState<'a> {
    known_stacks: HashMap<StackID, StackWithMetadata>,

    solana_pub_sub: SolanaPubSub<'a>,
    solana_get_stacks_config: RpcProgramAccountsConfig,
}

#[derive(Debug)]
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
            .post_and_reply(BlockchainMonitorMessage::Stop)
            .await
            .map_err(Into::into)
    }
}

pub async fn start(
    config: BlockchainMonitorConfig,
) -> Result<(
    impl BlockchainMonitor,
    UnboundedReceiver<BlockchainMonitorNotifications>,
)> {
    let (notification_channel, rx) = NotificationChannel::new();

    let get_stacks_config = RpcProgramAccountsConfig {
        filters: Some(vec![
            RpcFilterType::Memcmp(Memcmp {
                offset: 8,
                bytes: MemcmpEncodedBytes::Bytes(vec![5u8]),
                encoding: Some(MemcmpEncoding::Binary),
            }),
            RpcFilterType::Memcmp(Memcmp {
                offset: 8 + 1,
                bytes: MemcmpEncodedBytes::Bytes(
                    config
                        .solana_provider_public_key
                        .public_key
                        .to_bytes()
                        .to_vec(),
                ),
                encoding: Some(MemcmpEncoding::Binary),
            }),
            RpcFilterType::Memcmp(Memcmp {
                offset: 8 + 1 + 32 + 1,
                bytes: MemcmpEncodedBytes::Bytes(
                    config.solana_region_number.to_le_bytes().to_vec(),
                ),
                encoding: Some(MemcmpEncoding::Binary),
            }),
        ]),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            commitment: Some(CommitmentConfig::finalized()),
            ..Default::default()
        },
        with_context: Some(false),
    };

    let rpc_client = RpcClient::new_with_commitment(
        config.solana_cluster_rpc_url.clone(),
        CommitmentConfig::finalized(),
    );

    let existing_stacks = rpc_client
        .get_program_accounts_with_config(&marketplace::id(), get_stacks_config.clone())
        .await
        .context("Failed to fetch existing stacks from Solana")?;

    let existing_stacks = existing_stacks
        .into_iter()
        .map(read_solana_account)
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to parse stacks retrieved from Solana")?
        .into_iter()
        .map(|s| (s.id(), s))
        .collect::<HashMap<_, _>>();

    info!(
        "Received {} existing stacks from Solana",
        existing_stacks.len()
    );

    let solana_pub_sub = {
        let client_wrapper = Box::pin(SolanaPubSubClientWrapper {
            client: PubsubClient::new(&config.solana_cluster_pub_sub_url)
                .await
                .context("Failed to start Solana pub-sub client")?,
            _phantom_pinned: PhantomPinned,
        });

        let (subscription_stream, unsubscribe_fn) =
            unsafe { (client_wrapper.deref() as *const SolanaPubSubClientWrapper).as_ref() }
                .unwrap()
                .client
                .program_subscribe(&marketplace::id(), Some(get_stacks_config.clone()))
                .await
                .context("Failed to setup Solana subscription for new stacks")?;

        SolanaPubSub {
            client_wrapper,
            stream: subscription_stream,
            unsub_callback: unsubscribe_fn,
        }
    };

    let state = BlockchainMonitorState {
        known_stacks: existing_stacks,
        solana_get_stacks_config: get_stacks_config,
        solana_pub_sub,
    };

    let mailbox = PlainMailboxProcessor::start(
        |_mailbox, message_receiver| {
            mailbox_body(config, state, message_receiver, notification_channel)
        },
        10000,
    );

    // TODO: track deployed/undeployed stacks due to escrow balance
    Ok((BlockchainMonitorImpl { mailbox }, rx))
}

async fn mailbox_body(
    _config: BlockchainMonitorConfig,
    mut state: BlockchainMonitorState<'_>,
    mut message_receiver: MessageReceiver<BlockchainMonitorMessage>,
    notification_channel: NotificationChannel<BlockchainMonitorNotifications>,
) {
    notification_channel.send(BlockchainMonitorNotifications::StacksAvailable(
        state.known_stacks.values().cloned().collect(),
    ));

    let mut stop_reply_channel = None;

    'main_loop: loop {
        select! {
            message = message_receiver.receive() => {
                match message {
                    None => {
                        warn!("All senders were dropped, stopping");
                        break 'main_loop;
                    }

                    Some(BlockchainMonitorMessage::Stop(r)) => {
                        stop_reply_channel = Some(r);
                        break 'main_loop;
                    }

                    Some(BlockchainMonitorMessage::GetMetadata(stack_id, r)) => {
                        r.reply(
                            state.known_stacks.get(&stack_id).map(|s| s.metadata.clone())
                        );
                    }

                    Some(BlockchainMonitorMessage::GetStack(stack_id, r)) => {
                        r.reply(
                            state.known_stacks.get(&stack_id).map(Clone::clone)
                        );
                    }
                }
            }

            stack = state.solana_pub_sub.stream.next() => {
                if let Some(stack) = stack {
                    on_new_stack_received(&mut state, stack, &notification_channel);
                } else {
                    warn!("Solana notification stream disconnected, attempting to reconnect");
                    // TODO: this will make the mailbox stop processing messages while waiting to reconnect
                    // should probably handle subscriptions on a separate task
                    state = reconnect_solana_subscriber(state).await;
                }
            }
        }
    }

    (state.solana_pub_sub.unsub_callback)().await;

    if let Some(r) = stop_reply_channel {
        r.reply(());
    }
}

async fn reconnect_solana_subscriber(
    state: BlockchainMonitorState<'_>,
) -> BlockchainMonitorState<'_> {
    (state.solana_pub_sub.unsub_callback)().await;

    let (stream, unsub) = loop {
        let client_wrapper = unsafe {
            (state.solana_pub_sub.client_wrapper.deref() as *const SolanaPubSubClientWrapper)
                .as_ref()
        }
        .unwrap();

        match client_wrapper
            .client
            .program_subscribe(
                &marketplace::id(),
                Some(state.solana_get_stacks_config.clone()),
            )
            .await
            .context("Failed to re-setup Solana subscription for new stacks")
        {
            Ok(x) => break x,
            Err(f) => warn!("{f}"),
        }
    };

    BlockchainMonitorState {
        solana_pub_sub: SolanaPubSub {
            stream,
            unsub_callback: unsub,
            ..state.solana_pub_sub
        },
        ..state
    }
}

fn on_new_stack_received(
    state: &mut BlockchainMonitorState,
    stack: Response<RpcKeyedAccount>,
    notification_channel: &NotificationChannel<BlockchainMonitorNotifications>,
) {
    match read_solana_rpc_keyed_account(stack) {
        Err(f) => {
            warn!("Received stack from blockchain but failed to deserialize due to {f}");
        }

        Ok(stack) => {
            state.known_stacks.insert(stack.id(), stack.clone());
            notification_channel.send(BlockchainMonitorNotifications::StacksAvailable(vec![stack]));
        }
    }
}

fn read_solana_account((pubkey, account): (Pubkey, Account)) -> Result<StackWithMetadata> {
    let stack_data = marketplace::Stack::deserialize(&mut account.data.as_ref())
        .context("Failed to deserialize Stack data")?;

    let stack_definition =
        mu_stack::Stack::try_deserialize_proto(stack_data.stack.into_boxed_slice().as_ref())
            .context("Failed to deserialize stack definition")?;

    // TODO: state
    Ok(StackWithMetadata {
        stack: stack_definition,
        revision: stack_data.revision,
        metadata: StackMetadata::Solana(super::SolanaStackMetadata {
            account_id: pubkey,
            owner: stack_data.user,
        }),
        state: super::StackState::Normal,
    })
}

fn read_solana_rpc_keyed_account(stack: Response<RpcKeyedAccount>) -> Result<StackWithMetadata> {
    let pubkey = stack
        .value
        .pubkey
        .parse()
        .context("Failed to parse public key")?;
    let account = stack
        .value
        .account
        .decode()
        .ok_or_else(|| anyhow!("Failed to decode Account"))?;
    read_solana_account((pubkey, account))
}
