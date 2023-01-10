mod stack_collection;

use std::rc::Rc;
use std::time::{Duration, SystemTime};
use std::{collections::HashMap, marker::PhantomPinned, ops::Deref, pin::Pin};

use anchor_client::anchor_lang::AccountDeserialize;
use anchor_client::{Cluster, Program};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use futures::{future::BoxFuture, stream::BoxStream, StreamExt};
use itertools::Itertools;
use log::{debug, error, info, trace, warn};
use mailbox_processor::{
    plain::{MessageReceiver, PlainMailboxProcessor},
    NotificationChannel, ReplyChannel,
};
use marketplace::ServiceUsage;
use mu_stack::StackID;
use serde::Deserialize;
use solana_account_decoder::parse_token::{parse_token, TokenAccountType};
use solana_account_decoder::{UiAccount, UiAccountEncoding};
use solana_client::{
    nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient},
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, MemcmpEncoding, RpcFilterType},
    rpc_response::{Response, RpcKeyedAccount},
};
use solana_sdk::account::ReadableAccount;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::{
    account::Account, commitment_config::CommitmentConfig, pubkey::Pubkey, system_program,
};
use tokio::{select, sync::mpsc::UnboundedReceiver, task::spawn_blocking};

use super::{config_types::Base58PublicKey, StackMetadata, StackWithMetadata};
use crate::infrastructure::config::ConfigDuration;
use crate::stack::blockchain_monitor::stack_collection::{OwnerEntry, OwnerState, StackCollection};
use crate::stack::config_types::Base58PrivateKey;
use crate::stack::usage_aggregator::{UsageAggregator, UsageCategory};
use crate::stack::StackOwner;

// TODO: monitor for removed/undeployed stacks

#[async_trait]
#[clonable]
pub trait BlockchainMonitor: Clone + Send + Sync {
    async fn get_stack(&self, stack_id: StackID) -> Result<Option<StackWithMetadata>>;
    async fn get_metadata(&self, stack_id: StackID) -> Result<Option<StackMetadata>>;
    async fn stop(&self) -> Result<()>;
}

pub enum BlockchainMonitorNotification {
    StacksAvailable(Vec<StackWithMetadata>),
    StacksRemoved(Vec<StackWithMetadata>),
}

#[derive(Deserialize)]
pub struct BlockchainMonitorConfig {
    solana_cluster_rpc_url: String,
    solana_cluster_pub_sub_url: String,
    solana_provider_public_key: Base58PublicKey,
    solana_region_number: u32,
    solana_usage_signer_private_key: Base58PrivateKey,
    solana_min_escrow_balance: f64,
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

struct SolanaSubscription<'a, Account> {
    stream: BoxStream<'a, Response<Account>>,
    unsubscribe_callback: SolanaUnsubscribeFn,
}

struct SolanaPubSub<'a> {
    client_wrapper: Pin<Box<SolanaPubSubClientWrapper>>,

    get_stacks_config: RpcProgramAccountsConfig,
    stack_subscription: SolanaSubscription<'a, RpcKeyedAccount>,

    // Owner ID -> subscription
    escrow_subscriptions: HashMap<Pubkey, SolanaSubscription<'a, UiAccount>>,
}

struct Solana<'a> {
    rpc_client: RpcClient,
    pub_sub: SolanaPubSub<'a>,
    region_pda: Pubkey,
    provider_pda: Pubkey,
    token_decimals: u8,
}

struct State<'a> {
    stacks: StackCollection,
    solana: Solana<'a>,
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
    Vec<u8>,
)> {
    info!("Starting blockchain monitor");

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

    debug!(
        "Solana cluster URLs: {}, {}",
        config.solana_cluster_rpc_url, config.solana_cluster_pub_sub_url
    );

    let rpc_client = RpcClient::new_with_commitment(
        config.solana_cluster_rpc_url.clone(),
        CommitmentConfig::finalized(),
    );

    debug!("Verifying provider public key and region number");
    ensure_region_exists(&region_pda, &rpc_client).await?;

    debug!("Retrieving $MU token properties");
    let solana_token_decimals = get_token_decimals(&rpc_client).await?;
    debug!("$MU has {solana_token_decimals} decimal places");

    debug!("Setting up stack subscription");
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

    let mut solana_pub_sub = {
        let client_wrapper = Box::pin(SolanaPubSubClientWrapper {
            client: PubsubClient::new(&config.solana_cluster_pub_sub_url)
                .await
                .context("Failed to start Solana pub-sub client")?,
            _phantom_pinned: PhantomPinned,
        });

        let (stream, unsubscribe_callback) =
            unsafe { (client_wrapper.deref() as *const SolanaPubSubClientWrapper).as_ref() }
                .unwrap()
                .client
                .program_subscribe(&marketplace::id(), Some(get_stacks_config.clone()))
                .await
                .context("Failed to setup Solana subscription for new stacks")?;

        SolanaPubSub {
            client_wrapper,
            get_stacks_config: get_stacks_config.clone(),
            stack_subscription: SolanaSubscription {
                stream,
                unsubscribe_callback,
            },
            escrow_subscriptions: HashMap::new(),
        }
    };

    debug!("Retrieving existing stacks");
    let existing_stacks = rpc_client
        .get_program_accounts_with_config(&marketplace::id(), get_stacks_config)
        .await
        .context("Failed to fetch existing stacks from Solana")?;

    let existing_stacks = existing_stacks
        .into_iter()
        .map(read_solana_account)
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to parse stacks retrieved from Solana")?;

    info!(
        "Received {} existing stacks from Solana",
        existing_stacks.len()
    );

    let (solana_provider_pda, _) = Pubkey::find_program_address(
        &[
            b"provider",
            &config.solana_provider_public_key.public_key.to_bytes(),
        ],
        &marketplace::id(),
    );

    debug!("Fetching developer escrow balances");
    let owner_states =
        get_owner_states(&rpc_client, &solana_provider_pda, &config, existing_stacks).await?;
    let stacks = StackCollection::from_known(owner_states);

    debug!("Setting up escrow subscriptions");
    solana_pub_sub.escrow_subscriptions = setup_solana_escrow_subscriptions(
        unsafe {
            (solana_pub_sub.client_wrapper.deref() as *const SolanaPubSubClientWrapper).as_ref()
        }
        .unwrap(),
        &solana_provider_pda,
        stacks.owners().map(|o| match o {
            StackOwner::Solana(pk) => pk,
        }),
    )
    .await?;

    let state = State {
        stacks,
        solana: Solana {
            rpc_client,
            pub_sub: solana_pub_sub,
            provider_pda: solana_provider_pda,
            token_decimals: solana_token_decimals,
            region_pda,
        },
        usage_aggregator,
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

    debug!("Initialization complete");
    Ok((Box::new(res), rx, region_pda.to_bytes().into()))
}

async fn get_owner_states(
    rpc_client: &RpcClient,
    provider_pda: &Pubkey,
    config: &BlockchainMonitorConfig,
    stacks: impl IntoIterator<Item = StackWithMetadata>,
) -> Result<HashMap<StackOwner, (OwnerState, Vec<StackWithMetadata>)>> {
    let by_owner = stacks.into_iter().group_by(|s| s.owner());

    let mut res = HashMap::new();

    for (owner, stacks) in &by_owner {
        let escrow_balance = fetch_owner_escrow_balance(rpc_client, &owner, provider_pda).await?;

        let state = if escrow_balance >= config.solana_min_escrow_balance {
            OwnerState::Active
        } else {
            OwnerState::Inactive
        };

        trace!("Developer {owner:?} has escrow balance {escrow_balance} and state {state:?}");
        res.insert(owner, (state, stacks.collect()));
    }

    Ok(res)
}

async fn fetch_owner_escrow_balance(
    rpc_client: &RpcClient,
    owner: &StackOwner,
    provider_pda: &Pubkey,
) -> Result<f64> {
    let StackOwner::Solana(owner_key) = owner;
    //b"escrow", user.key().as_ref(), provider.key().as_ref()
    let (escrow_pda, _) = Pubkey::find_program_address(
        &[b"escrow", &owner_key.to_bytes(), &provider_pda.to_bytes()],
        &marketplace::id(),
    );
    let token_balance = rpc_client
        .get_token_account_balance(&escrow_pda)
        .await
        .context("Failed to fetch escrow balance from Solana")?;
    token_balance
        .ui_amount
        .context("Failed to get amount from token account")
}

async fn get_token_decimals(rpc_client: &RpcClient) -> Result<u8> {
    let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &marketplace::id());
    let state = rpc_client
        .get_account(&state_pda)
        .await
        .context("Failed to fetch mu state from Solana")?;
    let state = marketplace::MuState::try_deserialize(&mut state.data())
        .context("Failed to read mu state from Solana")?;

    let mint_address = state.mint;
    let mint = rpc_client
        .get_account(&mint_address)
        .await
        .context("Failed to fetch $MU mint from Solana")?;
    let mint = parse_token(mint.data(), None).context("Failed to read $MU mint from Solana")?;

    if let TokenAccountType::Mint(mint) = mint {
        Ok(mint.decimals)
    } else {
        bail!("Expected $MU mint to be a mint account");
    }
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
    mut state: State<'_>,
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

            stack = state.solana.pub_sub.stack_subscription.stream.next() => {
                if let Some(stack) = stack {
                    debug!("Received new stack");
                    if let Err(f) = on_new_stack_received(
                        &mut state,
                        &config,
                        stack,
                        &notification_channel
                    ).await {
                        // TODO: retry
                        warn!("Failed to process new stack: {f}");
                    }
                } else {
                    warn!("Solana notification stream disconnected, attempting to reconnect");
                    // TODO: this will make the mailbox stop processing messages while waiting to reconnect
                    // should probably handle subscriptions on a separate task
                    state = reconnect_solana_subscriber(state).await;
                }
            }

            escrow = select_next_escrow_update(
                &mut state.solana.pub_sub.escrow_subscriptions,
                state.solana.token_decimals
            ) => {
                match escrow {
                    Err(f) => warn!("Failed to receive escrow update: {f}"),
                    Ok(None) => {
                        warn!("Solana escrow update stream disconnected, attempting to reconnect");
                        // TODO: this will make the mailbox stop processing messages while waiting to reconnect
                        // should probably handle subscriptions on a separate task
                        state = reconnect_solana_subscriber(state).await;
                    },
                    Ok(Some((owner_pubkey, escrow_balance))) =>
                        on_solana_escrow_updated(
                            &mut state,
                            &config,
                            &notification_channel,
                            owner_pubkey,
                            escrow_balance
                        ),
                }
            }
        }
    }

    debug!("Will report usages one last time before stopping");
    if let Err(e) = report_usages(&mut state, &config).await {
        // TODO: this is a bad situation to be in, unless we persist usages to disk.
        error!("Failed to report usages due to: {e}");
    }
    (state.solana.pub_sub.stack_subscription.unsubscribe_callback)().await;

    if let Some(r) = stop_reply_channel {
        r.reply(());
    }
}

async fn select_next_escrow_update(
    subs: &mut HashMap<Pubkey, SolanaSubscription<'_, UiAccount>>,
    token_decimals: u8,
) -> Result<Option<(Pubkey, f64)>> {
    let next = futures::future::select_all(
        subs.iter_mut()
            .map(|x| Box::pin(next_escrow_update(x, token_decimals))),
    )
    .await;
    next.0
}

async fn next_escrow_update(
    sub: (&Pubkey, &mut SolanaSubscription<'_, UiAccount>),
    token_decimals: u8,
) -> Result<Option<(Pubkey, f64)>> {
    let account = sub.1.stream.next().await;

    if let Some(account) = account {
        trace!("Received escrow update for {}", sub.0);

        let account: Account = account
            .value
            .decode()
            .ok_or_else(|| anyhow!("Failed to parse account data from escrow update"))?;

        let account = parse_token(account.data(), Some(token_decimals))
            .context("Failed to parse token account from escrow update")?;

        if let TokenAccountType::Account(account) = account {
            let amount = account.token_amount.ui_amount.ok_or_else(|| {
                anyhow!(
                    "Failed to get UI amount from token account, this is likely \
                        due to too many decimal places in $MU"
                )
            })?;
            trace!("New escrow amount for {} is {amount}", sub.0);
            Ok(Some((*sub.0, amount)))
        } else {
            bail!("Expected escrow update to be a token account");
        }
    } else {
        Ok(None)
    }
}

fn on_solana_escrow_updated(
    state: &mut State,
    config: &BlockchainMonitorConfig,
    notification_channel: &NotificationChannel<BlockchainMonitorNotification>,
    owner_pubkey: Pubkey,
    escrow_balance: f64,
) {
    let new_state = if escrow_balance >= config.solana_min_escrow_balance {
        OwnerState::Active
    } else {
        OwnerState::Inactive
    };

    trace!("Developer {owner_pubkey} should be in state {new_state:?} due to escrow update");

    let owner = StackOwner::Solana(owner_pubkey);
    let owner_entry = state.stacks.owner_entry(owner.clone());
    match owner_entry {
        OwnerEntry::Vacant(_) => {
            warn!("Received escrow update for unknown developer {owner_pubkey}");
        }

        OwnerEntry::Occupied(occ) => {
            let old_state = occ.owner_state();
            if old_state != new_state {
                let stacks = occ.stacks().cloned().collect::<Vec<_>>();

                match new_state {
                    OwnerState::Active => {
                        trace!(
                            "Transitioning {owner_pubkey} to active state, \
                            stacks will be deployed for this owner"
                        );
                        state.stacks.make_active(&owner);
                        notification_channel
                            .send(BlockchainMonitorNotification::StacksAvailable(stacks));
                    }

                    OwnerState::Inactive => {
                        trace!(
                            "Transitioning {owner_pubkey} to inactive state, \
                            stacks will be undeployed for this owner"
                        );
                        state.stacks.make_inactive(&owner);
                        notification_channel
                            .send(BlockchainMonitorNotification::StacksRemoved(stacks));
                    }
                }
            } else {
                trace!("Already in desired state");
            }
        }
    }
}

async fn report_usages<'a>(state: &mut State<'a>, config: &BlockchainMonitorConfig) -> Result<()> {
    let usages = state.usage_aggregator.get_and_reset_usages().await?;
    let region_pda = state.solana.region_pda;
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

    debug!("Will report {} usages", usages.len());

    spawn_blocking(move || {
        let program_id = marketplace::id();

        let payer: Rc<dyn Signer> = Rc::new(signer_private_key);
        let program =
            anchor_client::Client::new(Cluster::Custom(rpc_url, pub_sub_url), payer.clone())
                .program(program_id);

        let (auth_signer_pda, _) = Pubkey::find_program_address(
            &[b"authorized_signer", region_pda.to_bytes().as_slice()],
            &program_id,
        );
        let auth_signer = program
            .account::<marketplace::AuthorizedUsageSigner>(auth_signer_pda)
            .context("Failed to load authorized usage signer from Solana")?;

        let (provider_pda, _) =
            Pubkey::find_program_address(&[b"provider", &provider_pubkey.to_bytes()], &program_id);

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

            trace!("Stack {stack_id} has total usage {usage:?}");

            if let Err(e) = report_usage(
                &program,
                payer.clone(),
                solana_stack_id,
                auth_signer.token_account,
                usage,
                provider_pda,
                region_pda,
                auth_signer_pda,
            ) {
                // TODO: need some way to keep the usage around for later
                error!("Failed to report usage for {stack_id} due to: {e:?}");
            }
        }

        Ok(())
    })
    .await
    .context("spawn_blocking failed")?
}

#[allow(clippy::too_many_arguments)]
fn report_usage(
    program: &Program,
    payer: Rc<dyn Signer>,
    stack_id: Pubkey,
    token_account: Pubkey,
    usage: ServiceUsage,
    provider_pda: Pubkey,
    region_pda: Pubkey,
    auth_signer_pda: Pubkey,
) -> Result<()> {
    let program_id = marketplace::id();
    let (state_pda, _) = Pubkey::find_program_address(&[b"state"], &program_id);

    let stack = program
        .account::<marketplace::Stack>(stack_id)
        .context("Failed to fetch stack from Solana")?;

    let (escrow_pda, escrow_bump) = Pubkey::find_program_address(
        &[b"escrow", &stack.user.to_bytes(), &provider_pda.to_bytes()],
        &program_id,
    );

    let seed: u128 = generate_seed();
    trace!("Report seed for stack {stack_id} is {seed}");
    let (usage_update_pda, _) = Pubkey::find_program_address(
        &[
            b"update",
            &stack_id.to_bytes(),
            &region_pda.to_bytes(),
            &seed.to_le_bytes(),
        ],
        &program_id,
    );

    let accounts = marketplace::accounts::UpdateUsage {
        authorized_signer: auth_signer_pda,
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
            update_seed: seed,
            usage,
        })
        .signer(payer.as_ref())
        .send()
        .context("Failed to send usage update transaction")?;

    Ok(())
}

fn generate_seed() -> u128 {
    // Note: the cast to u64 will overflow in around 584 millennia. Someone will have fixed it
    // by then.
    // We use a timestamp for the upper 64 bits to be able to sort usage updates based on their
    // seeds. May (or may not) come in handy later.
    let micros = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64;
    let rand: u64 = rand::random();
    (micros as u128) << 64 | (rand as u128)
}

// TODO: if the connection fails irrecoverably (such as by stopping the local validator),
// this gets called repetitively and prevents the application from quitting cleanly.
async fn reconnect_solana_subscriber(state: State<'_>) -> State<'_> {
    debug!("Reconnecting solana subscriptions");

    (state.solana.pub_sub.stack_subscription.unsubscribe_callback)().await;

    let client_wrapper = unsafe {
        (state.solana.pub_sub.client_wrapper.deref() as *const SolanaPubSubClientWrapper).as_ref()
    }
    .unwrap();

    let (stack_subscription, escrow_subscriptions) = setup_solana_subscriptions(
        client_wrapper,
        &state.solana.pub_sub.get_stacks_config,
        &state.solana.provider_pda,
        state.stacks.owners().map(|o| match o {
            StackOwner::Solana(pk) => pk,
        }),
    )
    .await;

    State {
        solana: Solana {
            pub_sub: SolanaPubSub {
                stack_subscription,
                escrow_subscriptions,
                ..state.solana.pub_sub
            },
            ..state.solana
        },
        ..state
    }
}

async fn setup_solana_subscriptions<'a>(
    pub_sub_client_wrapper: &'a SolanaPubSubClientWrapper,
    get_stacks_config: &RpcProgramAccountsConfig,
    provider_pda: &Pubkey,
    owners: impl Iterator<Item = &Pubkey> + Clone,
) -> (
    SolanaSubscription<'a, RpcKeyedAccount>,
    HashMap<Pubkey, SolanaSubscription<'a, UiAccount>>,
) {
    loop {
        let stack_subscription = match pub_sub_client_wrapper
            .client
            .program_subscribe(&marketplace::id(), Some(get_stacks_config.clone()))
            .await
            .context("Failed to re-setup Solana subscription for new stacks")
        {
            Ok((stream, unsubscribe_callback)) => SolanaSubscription {
                stream,
                unsubscribe_callback,
            },
            Err(f) => {
                warn!("{f}");
                continue;
            }
        };

        let escrow_subscriptions = match setup_solana_escrow_subscriptions(
            pub_sub_client_wrapper,
            provider_pda,
            owners.clone(),
        )
        .await
        {
            Ok(x) => x,
            Err(f) => {
                warn!("{f}");
                continue;
            }
        };

        return (stack_subscription, escrow_subscriptions);
    }
}

async fn setup_solana_escrow_subscriptions<'a>(
    pub_sub_client_wrapper: &'a SolanaPubSubClientWrapper,
    provider_pda: &Pubkey,
    owners: impl Iterator<Item = &Pubkey>,
) -> Result<HashMap<Pubkey, SolanaSubscription<'a, UiAccount>>> {
    let mut escrow_subscriptions = HashMap::<Pubkey, SolanaSubscription<'a, UiAccount>>::new();

    for owner_id in owners {
        trace!("Setting up escrow subscription for {owner_id}");

        let (escrow_pda, _) = Pubkey::find_program_address(
            &[b"escrow", &owner_id.to_bytes(), &provider_pda.to_bytes()],
            &marketplace::id(),
        );

        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            commitment: Some(CommitmentConfig::finalized()),
            ..Default::default()
        };

        let (stream, unsubscribe_callback) = pub_sub_client_wrapper
            .client
            .account_subscribe(&escrow_pda, Some(config))
            .await
            .context("Failed to setup Solana subscription for new stacks")?;

        escrow_subscriptions.insert(
            *owner_id,
            SolanaSubscription {
                stream,
                unsubscribe_callback,
            },
        );
    }

    Ok(escrow_subscriptions)
}

async fn on_new_stack_received(
    state: &mut State<'_>,
    config: &BlockchainMonitorConfig,
    stack: Response<RpcKeyedAccount>,
    notification_channel: &NotificationChannel<BlockchainMonitorNotification>,
) -> Result<()> {
    let stack = read_solana_rpc_keyed_account(stack)
        .context("Received stack from blockchain but failed to deserialize")?;

    debug!("Received new stack with ID {:?}", stack.id());

    // TODO: implement stack updates
    let owner_entry = state.stacks.owner_entry(stack.owner());

    let is_new_stack = match owner_entry {
        OwnerEntry::Occupied(mut occ) => {
            trace!(
                "Already know this stack's owner, which is in state {:?}",
                occ.owner_state()
            );
            occ.add_stack(stack.clone())
        }
        OwnerEntry::Vacant(vac) => {
            trace!("This stack is from a new owner, fetching escrow balance");
            let escrow_balance = fetch_owner_escrow_balance(
                &state.solana.rpc_client,
                &stack.owner(),
                &state.solana.provider_pda,
            )
            .await?;
            let state = if escrow_balance >= config.solana_min_escrow_balance {
                OwnerState::Active
            } else {
                OwnerState::Inactive
            };

            trace!("New owner has escrow balance {escrow_balance}, adding with state {state:?}");

            vac.insert_first(state, stack.clone());
            true
        }
    };

    if is_new_stack {
        notification_channel.send(BlockchainMonitorNotification::StacksAvailable(vec![stack]));
    }

    Ok(())
}

fn read_solana_account((pubkey, account): (Pubkey, Account)) -> Result<StackWithMetadata> {
    let stack_data = marketplace::Stack::try_deserialize(&mut &account.data[..])
        .context("Failed to deserialize Stack data")?;

    let stack_definition =
        mu_stack::Stack::try_deserialize_proto(stack_data.stack.into_boxed_slice().as_ref())
            .context("Failed to deserialize stack definition")?;

    Ok(StackWithMetadata {
        stack: stack_definition,
        revision: stack_data.revision,
        metadata: StackMetadata::Solana(super::SolanaStackMetadata {
            account_id: pubkey,
            owner: stack_data.user,
        }),
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
    debug!("Fetching region data from Solana");

    let account = rpc_client.get_account(region).await.context(format!(
        "Failed to fetch region {region} from Solana, make sure the `solana_provider_public_key` and \
            `solana_region_num` config values are correct, the region is already created, the URL in \
            `solana_cluster_rpc_url` is available and that you are not running into rate limits",
    ))?;

    // deserialize to ensure the account data is of the correct type
    let region =
        marketplace::ProviderRegion::try_deserialize(&mut &account.data[..]).context(format!(
            "Failed to deserialize region in {region}, ensure the region was deployed correctly"
        ))?;
    info!("Starting in region {}", region.name);

    Ok(())
}
