mod stack_collection;
mod vmdata;

use std::rc::Rc;
use std::time::{Duration, SystemTime};
use std::{collections::HashMap, marker::PhantomPinned, ops::Deref, pin::Pin};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use futures::{future::BoxFuture, stream::BoxStream, StreamExt};
use http::Uri;
use itertools::Itertools;
use log::{debug, error, info, trace, warn};
use mailbox_processor::{
    plain::{MessageReceiver, PlainMailboxProcessor},
    NotificationChannel, ReplyChannel,
};
use mu_common::pwr::VM_ID;
use mu_stack::{StackID, StackOwner, ValidatedStack};
use num::traits::ToBytes;
use pwr_rs::block::{NewTransactionData, Transaction, TransactionData};
use pwr_rs::rpc::RPC;
use pwr_rs::wallet::{PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};
use tokio::{select, sync::mpsc::UnboundedReceiver, task::spawn_blocking};
use uuid::Uuid;

use self::vmdata::NewStack;

use super::ApiRequestSigner;
use super::{StackMetadata, StackWithMetadata};
use crate::infrastructure::config::{ConfigDuration, ConfigUri};
use crate::stack;
use crate::stack::blockchain_monitor::stack_collection::{OwnerEntry, OwnerState, StackCollection};
use crate::stack::blockchain_monitor::vmdata::{ServiceUsage, VMData};
use crate::stack::usage_aggregator::{UsageAggregator, UsageCategory};

#[async_trait]
#[clonable]
pub trait BlockchainMonitor: Clone + Send + Sync {
    async fn get_stack(&self, stack_id: StackID) -> Result<Option<StackWithMetadata>>;
    async fn get_metadata(&self, stack_id: StackID) -> Result<Option<StackMetadata>>;
    async fn stop(&self) -> Result<()>;
}

pub enum BlockchainMonitorNotification {
    StacksAvailable(Vec<StackWithMetadata>),
    StacksRemoved(Vec<(StackID, StackRemovalMode)>),
    RequestSignersAvailable(Vec<(ApiRequestSigner, StackOwner)>),
    RequestSignersRemoved(Vec<ApiRequestSigner>),
}

#[derive(Debug, Clone, Copy)]
pub enum StackRemovalMode {
    Temporary,
    Permanent,
}

#[derive(Deserialize, Clone)]
pub struct BlockchainMonitorConfig {
    pwr_rpc_url: ConfigUri,
    pwr_start_block_number: u32,
    pwr_provider_public_key: PublicKey,
    pwr_usage_signer_private_key: PrivateKey,
    pwr_usage_report_interval: ConfigDuration,
}

struct State {
    stacks: StackCollection,
    pwr_rpc: pwr_rs::rpc::RPC,
    transaction_stream: tokio::sync::mpsc::UnboundedReceiver<ParsedVMData>,
    usage_aggregator: Box<dyn UsageAggregator>,
}

#[derive(Debug)]
enum BlockchainMonitorMessage {
    GetStack(StackID, ReplyChannel<Option<StackWithMetadata>>),
    GetMetadata(StackID, ReplyChannel<Option<StackMetadata>>),
    Tick(ReplyChannel<()>),
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
    usage_aggregator: Box<dyn UsageAggregator>,
) -> Result<(
    Box<dyn BlockchainMonitor>,
    UnboundedReceiver<BlockchainMonitorNotification>,
    RegionConfig,
)> {
    info!("Starting blockchain monitor");

    let (notification_channel, rx) = NotificationChannel::new();

    debug!("PWR RPC URL: {}", config.pwr_rpc_url.0);

    let (transaction_stream_tx, transaction_stream_rx) = tokio::sync::mpsc::unbounded_channel();

    let rpc_client = RPC::new(config.pwr_rpc_url.0.to_string())
        .map_err(|e| anyhow!("Failed to create rpc: {e}"))
        .unwrap();

    let state = State {
        stacks: StackCollection::new(),
        pwr_rpc: rpc_client,
        usage_aggregator,
        transaction_stream: transaction_stream_rx,
    };

    let tick_interval = *config.pwr_usage_report_interval;

    let mailbox = PlainMailboxProcessor::start(
        {
            let config = config.clone();
            move |_mailbox, message_receiver| {
                mailbox_body(config, state, message_receiver, notification_channel)
            }
        },
        10000,
    );

    let res = BlockchainMonitorImpl { mailbox };

    tokio::spawn({
        let res = res.clone();
        async move { generate_tick(res, tick_interval).await }
    });

    tokio::spawn({
        let config = config.clone();
        async move {
            pwr_transaction_monitor(
                transaction_stream_tx,
                config.pwr_rpc_url.0,
                config.pwr_start_block_number,
            )
            .await
        }
    });

    let region_config = RegionConfig {
        id: [0u8; 32].to_vec(), // Single Node for now
        max_giga_instructions_per_call: Some(10),
    };

    debug!("Initialization complete");
    Ok((Box::new(res), rx, region_config))
}

async fn generate_tick(blockchain_monitor: BlockchainMonitorImpl, interval: Duration) {
    let mut timer = tokio::time::interval(interval);
    // Timers tick once immediately
    timer.tick().await;

    loop {
        timer.tick().await;
        if let Err(mailbox_processor::Error::MailboxStopped) = blockchain_monitor
            .mailbox
            .post_and_reply(BlockchainMonitorMessage::Tick)
            .await
        {
            return;
        }
    }
}

async fn mailbox_body(
    config: BlockchainMonitorConfig,
    mut state: State,
    mut message_receiver: MessageReceiver<BlockchainMonitorMessage>,
    notification_channel: NotificationChannel<BlockchainMonitorNotification>,
) {
    if state.stacks.all_active().next().is_some() {
        notification_channel.send(BlockchainMonitorNotification::StacksAvailable(
            state.stacks.all_active().cloned().collect(),
        ));
    }

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
                        debug!("Stopping");
                        stop_reply_channel = Some(r);
                        break 'main_loop;
                    }

                    Some(BlockchainMonitorMessage::GetMetadata(stack_id, r)) => {
                        r.reply(
                            match state.stacks.entry(stack_id) {
                                stack_collection::Entry::Active(a) => Some(a.get().metadata.clone()),
                                _ => None
                            }
                        );
                    }

                    Some(BlockchainMonitorMessage::GetStack(stack_id, r)) => {
                        r.reply(
                            match state.stacks.entry(stack_id) {
                                stack_collection::Entry::Active(a) => Some(a.get().clone()),
                                _ => None
                            }
                        );
                    }

                    Some(BlockchainMonitorMessage::Tick(r)) => {
                        r.reply(());

                        debug!("Reporting usages");
                        if let Err(e) = report_usages(&mut state, &config).await {
                            error!("Failed to report usages due to: {e}");
                        }
                    }
                }
            }

            data = state.transaction_stream.recv() => {
                if let Some(d) = data {
                    match d {
                        ParsedVMData::Usage(_) => {},
                        ParsedVMData::Stack(stack) => {
                            debug!("Received new stack");
                            if let Err(f) = on_new_stack_received(
                                &mut state,
                                stack,
                                &notification_channel
                            ).await {
                                warn!("Failed to process new stack: {f}");
                            }
                        }
                    }
                } else {
                    warn!("PWR transaction stream disconnected");
                }
            }
        }
    }

    debug!("Will report usages one last time before stopping");
    if let Err(e) = report_usages(&mut state, &config).await {
        // TODO: this is a bad situation to be in, unless we persist usages to disk.
        error!("Failed to report usages due to: {e}");
    }

    if let Some(r) = stop_reply_channel {
        r.reply(());
    }
}

async fn report_usages(state: &mut State, config: &BlockchainMonitorConfig) -> Result<()> {
    let usages = state.usage_aggregator.get_and_reset_usages().await?;

    debug!("Will report {} usages", usages.len());

    for (stack_id, usages) in usages {
        let stack_id = match stack_id {
            StackID::PWRStackID(x) => x,
        };
        let mut usage = ServiceUsage::default();
        for (category, amount) in usages {
            match category {
                UsageCategory::FunctionMBInstructions => usage.function_mb_instructions = amount,
                UsageCategory::DBStorage => usage.db_bytes_seconds = amount,
                UsageCategory::DBReads => usage.db_reads = amount as u64,
                UsageCategory::DBWrites => usage.db_writes = amount as u64,
                UsageCategory::GatewayRequests => usage.gateway_requests = amount as u64,
                UsageCategory::GatewayTraffic => usage.gateway_traffic_bytes = amount as u64,
            }
        }

        trace!("Stack {stack_id} has total usage {usage:?}");

        let data = VMData::Usage(usage);
        let trx_data = NewTransactionData::VmData {
            vm_id: VM_ID,
            data: bincode::serialize(&data).unwrap(),
        };

        if let Err(e) = state
            .pwr_rpc
            .broadcast_transaction(&trx_data, &config.pwr_usage_signer_private_key)
            .await
        {
            // TODO: need some way to keep the usage around for later
            error!("Failed to report usage for {stack_id} due to: {e:?}");
        }
    }

    Ok(())
}

pub struct RegionConfig {
    pub id: Vec<u8>,
    pub max_giga_instructions_per_call: Option<u32>,
}

async fn on_new_stack_received(
    state: &mut State,
    stack: StackWithMetadata,
    notification_channel: &NotificationChannel<BlockchainMonitorNotification>,
) -> Result<()> {
    debug!("Received new stack with ID {:?}", stack.id());

    let owner_entry = state.stacks.owner_entry(stack.owner());

    let should_report_stack = match owner_entry {
        OwnerEntry::Occupied(mut occ) => {
            trace!(
                "Already know this stack's owner, which is in state {:?}",
                occ.owner_state()
            );

            occ.add_stack(stack.clone())
        }
        OwnerEntry::Vacant(vac) => {
            trace!("This stack is from a new owner");
            vac.insert_first(OwnerState::Active, stack.clone());
            true
        }
    };

    if should_report_stack {
        notification_channel.send(BlockchainMonitorNotification::StacksAvailable(vec![stack]));
    }

    Ok(())
}

enum ParsedVMData {
    Stack(StackWithMetadata),
    Usage(ServiceUsage),
}

fn read_pwr_vmdata(block_number: u32, transaction_idx: u32, data: Vec<u8>) -> Result<ParsedVMData> {
    let data: VMData = bincode::deserialize(&data).context("Parse vmdata")?;
    match data {
        VMData::NewStack(s) => {
            read_pwr_stack_data(block_number, transaction_idx, s).map(ParsedVMData::Stack)
        }
        VMData::Usage(u) => Ok(ParsedVMData::Usage(u)),
    }
}

fn read_pwr_stack_data(
    block_number: u32,
    transaction_idx: u32,
    stack: NewStack,
) -> Result<StackWithMetadata> {
    let stack_definition = mu_stack::Stack::try_deserialize_proto(stack.stack_data)
        .context("Failed to deserialize stack definition")?;

    let validated_stack = stack_definition
        .validate()
        .map_err(|(_, e)| e)
        .context("Invalid stack definition")?;

    let trx_bytes = transaction_idx.to_be_bytes();

    Ok(StackWithMetadata {
        stack: validated_stack,
        name: stack.name,
        revision: stack.revision,
        metadata: StackMetadata::PWR(super::PWRStackMetadata {
            stack_id: Uuid::from_fields(
                block_number,
                u16::from_be_bytes([trx_bytes[0], trx_bytes[1]]),
                u16::from_be_bytes([trx_bytes[2], trx_bytes[3]]),
                &[0u8; 8],
            ),
            owner: stack.owner,
        }),
    })
}

async fn pwr_transaction_monitor(
    transaction_stream_sender: tokio::sync::mpsc::UnboundedSender<ParsedVMData>,
    pwr_rpc_url: Uri,
    start_block_number: u32,
) {
    let rpc_client = RPC::new(pwr_rpc_url.to_string())
        .map_err(|e| anyhow!("Failed to create rpc: {e}"))
        .unwrap();

    let mut latest_block_number;
    let mut block_number = start_block_number as _;

    loop {
        latest_block_number = rpc_client.latest_block_count().await.unwrap();
        while block_number < latest_block_number {
            let block = match rpc_client.block_by_number(block_number).await {
                Ok(b) => b,
                Err(e) => {
                    error!("Failed to read block `{block_number}`: {e:?}");
                    block_number += 1;
                    continue;
                }
            };

            for t in block.transactions {
                if let TransactionData::VmData { vm_id, data } = t.data {
                    if vm_id == VM_ID {
                        match read_pwr_vmdata(block.block_number, t.position_in_the_block, data) {
                            Err(e) => error!("Invalid vmdata: {e}"),
                            Ok(data) => {
                                if let Err(e) = transaction_stream_sender.send(data) {
                                    error!("Failed to send data to transaction_stream: {e}");
                                }
                            }
                        }
                    }
                }
            }
            block_number += 1;
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
