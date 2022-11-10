use std::rc::Rc;
use std::time::Duration;
use std::{collections::HashMap, marker::PhantomPinned, ops::Deref, pin::Pin};

use crate::infrastructure::config::ConfigDuration;
use crate::stack::config_types::Base58PrivateKey;
use crate::stack::usage_aggregator::{UsageAggregator, UsageCategory};
use anchor_client::anchor_lang::AccountDeserialize;
use anchor_client::{Cluster, Program};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use futures::{future::BoxFuture, stream::BoxStream, StreamExt};
use log::{error, info, warn};
use mailbox_processor::{
    plain::{MessageReceiver, PlainMailboxProcessor},
    NotificationChannel, ReplyChannel,
};
use marketplace::ServiceUsage;
use mu_stack::StackID;
use serde::Deserialize;
use solana_account_decoder::UiAccountEncoding;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_client::{
    nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient},
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, MemcmpEncoding, RpcFilterType},
    rpc_response::{Response, RpcKeyedAccount},
};
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::{
    account::Account, commitment_config::CommitmentConfig, pubkey::Pubkey, system_program,
};
use tokio::{select, sync::mpsc::UnboundedReceiver, task::spawn_blocking};

use super::{config_types::Base58PublicKey, StackMetadata, StackWithMetadata};

//TODO: usage updates and escrow
#[async_trait]
#[clonable]
pub trait BlockchainMonitor: Clone + Send + Sync {
    async fn get_stack(&self, stack_id: StackID) -> Result<Option<StackWithMetadata>>;
    async fn get_metadata(&self, stack_id: StackID) -> Result<Option<StackMetadata>>;
    async fn stop(&self) -> Result<()>;
}

pub enum BlockchainMonitorNotification {
    // TODO: monitor for removed/undeployed stacks
    StacksAvailable(Vec<StackWithMetadata>),
}

#[derive(Deserialize)]
pub struct BlockchainMonitorConfig {
    solana_cluster_rpc_url: String,
    solana_cluster_pub_sub_url: String,
    solana_provider_public_key: Base58PublicKey,
    solana_region_number: u32,
    solana_usage_signer_private_key: Base58PrivateKey,
    // solana_min_escrow_balance: u64,
    solana_usage_report_interval: ConfigDuration,
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
    unsubscribe_callback: SolanaUnsubscribeFn,
}

struct BlockchainMonitorState<'a> {
    known_stacks: HashMap<StackID, StackWithMetadata>,

    solana_pub_sub: SolanaPubSub<'a>,
    solana_get_stacks_config: RpcProgramAccountsConfig,
    solana_region_pda: Pubkey,

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
)> {
    let (notification_channel, rx) = NotificationChannel::new();

    let (region_pda, _) = Pubkey::find_program_address(
        &[
            "region".as_bytes(),
            config
                .solana_provider_public_key
                .public_key
                .to_bytes()
                .as_ref(),
            config.solana_region_number.to_le_bytes().as_ref(),
        ],
        &marketplace::id(),
    );

    let rpc_client = RpcClient::new_with_commitment(
        config.solana_cluster_rpc_url.clone(),
        CommitmentConfig::finalized(),
    );

    ensure_region_exists(&region_pda, &rpc_client).await?;

    let get_stacks_config = RpcProgramAccountsConfig {
        filters: Some(vec![
            RpcFilterType::Memcmp(Memcmp {
                offset: 8,
                bytes: MemcmpEncodedBytes::Bytes(vec![5u8]),
                encoding: Some(MemcmpEncoding::Binary),
            }),
            RpcFilterType::Memcmp(Memcmp {
                offset: 8 + 1 + 32,
                bytes: MemcmpEncodedBytes::Bytes(region_pda.to_bytes().to_vec()),
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
            unsubscribe_callback: unsubscribe_fn,
        }
    };

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

    let state = BlockchainMonitorState {
        known_stacks: existing_stacks,
        solana_get_stacks_config: get_stacks_config,
        solana_pub_sub,
        usage_aggregator,
        solana_region_pda: region_pda,
    };

    let tick_interval = *config.solana_usage_report_interval;

    let mailbox = PlainMailboxProcessor::start(
        |_mailbox, message_receiver| {
            mailbox_body(config, state, message_receiver, notification_channel)
        },
        10000,
    );

    let res = BlockchainMonitorImpl { mailbox };

    let res_clone = res.clone();
    tokio::spawn(async move { generate_tick(res_clone, tick_interval).await });

    // TODO: track deployed/undeployed stacks due to escrow balance
    Ok((Box::new(res), rx))
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
    mut state: BlockchainMonitorState<'_>,
    mut message_receiver: MessageReceiver<BlockchainMonitorMessage>,
    notification_channel: NotificationChannel<BlockchainMonitorNotification>,
) {
    if !state.known_stacks.is_empty() {
        notification_channel.send(BlockchainMonitorNotification::StacksAvailable(
            state.known_stacks.values().cloned().collect(),
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

                    Some(BlockchainMonitorMessage::Tick(r)) => {
                        r.reply(());

                        if let Err(e) = report_usages(&mut state, &config).await {
                            error!("Failed to report usages due to: {e}");
                        }
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

    if let Err(e) = report_usages(&mut state, &config).await {
        // TODO: this is a bad situation to be in, unless we persist usages to disk.
        error!("Failed to report usages due to: {e}");
    }
    (state.solana_pub_sub.unsubscribe_callback)().await;

    if let Some(r) = stop_reply_channel {
        r.reply(());
    }
}

async fn report_usages<'a>(
    state: &mut BlockchainMonitorState<'a>,
    config: &BlockchainMonitorConfig,
) -> Result<()> {
    let usages = state.usage_aggregator.get_and_reset_usages().await?;
    let region_pda = state.solana_region_pda;
    let provider_pubkey = config.solana_provider_public_key.public_key;
    let rpc_url = config.solana_cluster_rpc_url.clone();
    let pub_sub_url = config.solana_cluster_pub_sub_url.clone();
    let signer_private_key = Keypair::from_bytes(
        config
            .solana_usage_signer_private_key
            .keypair
            .to_bytes()
            .as_slice(),
    )
    .unwrap();

    spawn_blocking(move || {
        let payer: Rc<dyn Signer> = Rc::new(signer_private_key);
        let program =
            anchor_client::Client::new(Cluster::Custom(rpc_url, pub_sub_url), payer.clone())
                .program(marketplace::id());

        let (auth_signer_pda, _) = Pubkey::find_program_address(
            &[b"authorized_signer", region_pda.to_bytes().as_slice()],
            &marketplace::id(),
        );
        let auth_signer = program
            .account::<marketplace::AuthorizedUsageSigner>(auth_signer_pda)
            .context("Failed to load authorized usage signer from Solana")?;

        // TODO: currently, we must update usages per stack.
        // let mut usages_by_user = HashMap::new();
        //
        // for (stack_id, stack_usage) in usages {
        //     // TODO: this assumes we'll only use solana
        //     let user_id = match state.known_stacks.get(&stack_id) {
        //         None => {
        //             warn!("Have usage reports for unknown stack ID {stack_id}");
        //             continue;
        //         }
        //
        //         Some(stack) => match &stack.metadata {
        //             StackMetadata::Solana(s) => s.owner,
        //         },
        //     };
        //
        //     let user_usages = usages_by_user.entry(user_id).or_insert_with(HashMap::new);
        //
        //     for (category, amount) in stack_usage {
        //         let total = user_usages.entry(category).or_insert(0u128);
        //         *total += amount;
        //     }
        // }

        for (stack_id, usages) in usages {
            let solana_stack_id = match stack_id {
                StackID::SolanaPublicKey(x) => Pubkey::new_from_array(x),
            };
            let mut usage = marketplace::ServiceUsage::default();
            for (category, amount) in usages {
                match category {
                    UsageCategory::FunctionMBInstructions => {
                        usage.function_mb_instructions = amount
                    }
                    UsageCategory::DBStorage => usage.db_bytes_seconds = amount,
                    UsageCategory::DBReads => usage.db_reads = amount as u64,
                    UsageCategory::DBWrites => usage.db_writes = amount as u64,
                    UsageCategory::GatewayRequests => usage.gateway_requests = amount as u64,
                    UsageCategory::GatewayTraffic => usage.gateway_traffic_bytes = amount as u64,
                }
            }

            if let Err(e) = report_usage(
                &program,
                payer.clone(),
                solana_stack_id,
                auth_signer.token_account,
                usage,
                region_pda,
                provider_pubkey,
            ) {
                // TODO: need some way to keep the usage around for later
                error!("Failed to report usage for {stack_id} due to: {e}");
            }
        }

        Ok(())
    })
    .await
    .context("spawn_blocking failed")?
}

fn report_usage(
    program: &Program,
    payer: Rc<dyn Signer>,
    stack_id: Pubkey,
    token_account: Pubkey,
    usage: ServiceUsage,
    region_pda: Pubkey,
    provider_pubkey: Pubkey,
) -> Result<()> {
    let program_id = marketplace::id();
    let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &program_id);

    let stack = program
        .account::<marketplace::Stack>(stack_id)
        .context("Failed to fetch stack from Solana")?;

    let (escrow_pda, escrow_bump) = Pubkey::find_program_address(
        &[
            b"escrow",
            &stack.user.to_bytes(),
            &provider_pubkey.to_bytes(),
        ],
        &program_id,
    );

    let seed = 0u64; // TODO: we can't reliably generate consecutive seeds, make this a random UUID/16-byte number?
    let (usage_update_pda, _) =
        Pubkey::find_program_address(&[b"update", &seed.to_le_bytes()], &program_id);

    let accounts = marketplace::accounts::UpdateUsage {
        authorized_signer: payer.pubkey(),
        escrow_account: escrow_pda,
        region: region_pda,
        signer: payer.pubkey(),
        stack: stack_id,
        state: state_pda,
        token_program: spl_token::id(),
        system_program: system_program::id(),
        token_account,
        usage_update: usage_update_pda,
    };

    program
        .request()
        .accounts(accounts)
        .args(marketplace::instruction::UpdateUsage {
            _escrow_bump: escrow_bump,
            _update_seed: seed,
            usage,
        })
        .signer(payer.as_ref())
        .send_with_spinner_and_config(RpcSendTransactionConfig {
            // TODO: what's preflight and what's a preflight commitment?
            skip_preflight: cfg!(debug_assertions),
            ..Default::default()
        })
        .context("Failed to send usage update transaction")?;

    Ok(())
}

// TODO: if the connection fails irrecoverably, this gets called repetitively and prevents
// the application from quitting cleanly.
async fn reconnect_solana_subscriber(
    state: BlockchainMonitorState<'_>,
) -> BlockchainMonitorState<'_> {
    (state.solana_pub_sub.unsubscribe_callback)().await;

    let (stream, unsubscribe) = loop {
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
            unsubscribe_callback: unsubscribe,
            ..state.solana_pub_sub
        },
        ..state
    }
}

fn on_new_stack_received(
    state: &mut BlockchainMonitorState,
    stack: Response<RpcKeyedAccount>,
    notification_channel: &NotificationChannel<BlockchainMonitorNotification>,
) {
    match read_solana_rpc_keyed_account(stack) {
        Err(f) => {
            warn!("Received stack from blockchain but failed to deserialize due to {f}");
        }

        Ok(stack) => {
            // TODO: implement stack updates
            if state
                .known_stacks
                .insert(stack.id(), stack.clone())
                .is_none()
            {
                notification_channel
                    .send(BlockchainMonitorNotification::StacksAvailable(vec![stack]));
            }
        }
    }
}

fn read_solana_account((pubkey, account): (Pubkey, Account)) -> Result<StackWithMetadata> {
    let stack_data = marketplace::Stack::try_deserialize(&mut &account.data[..])
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

// TODO: ensure we have the right usage signer for this region
async fn ensure_region_exists(region: &Pubkey, rpc_client: &RpcClient) -> Result<()> {
    let account = rpc_client.get_account(region).await.context(format!(
        "Failed to fetch region {region} from Solana, make sure the `solana_provider_public_key` and \
            `solana_region_num` config values are correct and the region is already created",
    ))?;

    // deserialize to ensure the account data is of the correct type
    let _ = marketplace::ProviderRegion::try_deserialize(&mut &account.data[..]).context(
        format!("Failed to deserialize region {region}, ensure the region was deployed correctly"),
    )?;

    Ok(())
}
