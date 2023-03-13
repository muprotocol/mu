mod stack_collection;

use std::rc::Rc;
use std::time::{Duration, SystemTime};
use std::{collections::HashMap, marker::PhantomPinned, ops::Deref, pin::Pin};

use anchor_client::anchor_lang::{AccountDeserialize, Discriminator};
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
use mu_stack::{StackID, StackOwner};
use serde::Deserialize;
use solana_account_decoder::parse_token::{
    parse_token, token_amount_to_ui_amount, TokenAccountType,
};
use solana_account_decoder::{UiAccount, UiAccountEncoding};
use solana_client::client_error::{ClientError, ClientErrorKind};
use solana_client::rpc_request::RpcError;
use solana_client::{
    nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient},
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
    rpc_response::{Response, RpcKeyedAccount},
};
use solana_sdk::account::ReadableAccount;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::{
    account::Account, commitment_config::CommitmentConfig, pubkey::Pubkey, system_program,
};
use tokio::{select, sync::mpsc::UnboundedReceiver, task::spawn_blocking};

use super::ApiRequestSigner;
use super::{config_types::Base58PublicKey, StackMetadata, StackWithMetadata};
use crate::infrastructure::config::{ConfigDuration, ConfigUri};
use crate::stack::blockchain_monitor::stack_collection::{OwnerEntry, OwnerState, StackCollection};
use crate::stack::config_types::Base58PrivateKey;
use crate::stack::usage_aggregator::{UsageAggregator, UsageCategory};

#[async_trait]
#[clonable]
pub trait BlockchainMonitor: Clone + Send + Sync {
    async fn get_stack(&self, stack_id: StackID) -> Result<Option<StackWithMetadata>>;
    async fn get_metadata(&self, stack_id: StackID) -> Result<Option<StackMetadata>>;
    async fn get_escrow_balance(&self, owner: StackOwner) -> Result<Option<EscrowBalance>>;
    async fn stop(&self) -> Result<()>;
}

pub struct EscrowBalance {
    pub user_balance: f64,
    pub min_balance: f64,
}

impl EscrowBalance {
    pub fn is_over_minimum(&self) -> bool {
        self.user_balance > self.min_balance
    }
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

#[derive(Deserialize)]
pub struct BlockchainMonitorConfig {
    solana_cluster_rpc_url: ConfigUri,
    solana_cluster_pub_sub_url: ConfigUri,
    solana_provider_public_key: Base58PublicKey,
    solana_region_number: u32,
    solana_usage_signer_private_key: Base58PrivateKey,
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

    get_request_signers_config: RpcProgramAccountsConfig,
    request_signer_subscription: SolanaSubscription<'a, RpcKeyedAccount>,

    // Owner ID -> subscription
    escrow_subscriptions: HashMap<Pubkey, SolanaSubscription<'a, UiAccount>>,
}

struct Solana<'a> {
    rpc_client: RpcClient,
    pub_sub: SolanaPubSub<'a>,
    region_pda: Pubkey,
    provider_pda: Pubkey,
    token_decimals: u8,
    min_escrow_balance: u64,
    escrow_balances: HashMap<Pubkey, u64>,
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
    GetEscrowBalance(StackOwner, ReplyChannel<Option<EscrowBalance>>),
    Tick(ReplyChannel<()>),
    Stop(ReplyChannel<()>),
}

enum StackWithState {
    Active(StackWithMetadata),
    Deleted {
        stack_id: StackID,
        owner_id: StackOwner,
    },
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

    async fn get_escrow_balance(&self, owner: StackOwner) -> Result<Option<EscrowBalance>> {
        self.mailbox
            .post_and_reply(|r| BlockchainMonitorMessage::GetEscrowBalance(owner, r))
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
        config.solana_cluster_rpc_url.0,
        config.solana_cluster_pub_sub_url.0.to_string()
    );

    let rpc_client = RpcClient::new_with_commitment(
        config.solana_cluster_rpc_url.0.to_string(),
        CommitmentConfig::finalized(),
    );

    debug!("Verifying provider public key and region number");
    let region = get_region(&region_pda, &rpc_client).await?;

    debug!("Retrieving $MU token properties");
    let solana_token_decimals = get_token_decimals(&rpc_client).await?;
    debug!("$MU has {solana_token_decimals} decimal places");

    debug!("Setting up stack subscription");

    let get_stacks_config = RpcProgramAccountsConfig {
        filters: Some(vec![
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                0,
                marketplace::Stack::discriminator().to_vec(),
            )),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                8 + 32,
                region_pda.to_bytes().to_vec(),
            )),
        ]),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            commitment: Some(CommitmentConfig::finalized()),
            ..Default::default()
        },
        with_context: Some(false),
    };

    let get_request_signers_config = RpcProgramAccountsConfig {
        filters: Some(vec![
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                0,
                marketplace::ApiRequestSigner::discriminator().to_vec(),
            )),
            RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                8 + 32 + 32,
                region_pda.to_bytes().to_vec(),
            )),
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
            client: PubsubClient::new(&config.solana_cluster_pub_sub_url.0.to_string())
                .await
                .context("Failed to start Solana pub-sub client")?,
            _phantom_pinned: PhantomPinned,
        });

        let (stack_stream, stack_unsubscribe_callback) =
            unsafe { (client_wrapper.deref() as *const SolanaPubSubClientWrapper).as_ref() }
                .unwrap()
                .client
                .program_subscribe(&marketplace::id(), Some(get_stacks_config.clone()))
                .await
                .context("Failed to setup Solana subscription for new stacks")?;

        let (request_signer_stream, request_signer_unsubscribe_callback) =
            unsafe { (client_wrapper.deref() as *const SolanaPubSubClientWrapper).as_ref() }
                .unwrap()
                .client
                .program_subscribe(&marketplace::id(), Some(get_request_signers_config.clone()))
                .await
                .context("Failed to setup Solana subscription for new request signers")?;

        SolanaPubSub {
            client_wrapper,
            get_stacks_config: get_stacks_config.clone(),
            stack_subscription: SolanaSubscription {
                stream: stack_stream,
                unsubscribe_callback: stack_unsubscribe_callback,
            },
            get_request_signers_config: get_request_signers_config.clone(),
            request_signer_subscription: SolanaSubscription {
                stream: request_signer_stream,
                unsubscribe_callback: request_signer_unsubscribe_callback,
            },
            escrow_subscriptions: HashMap::new(),
        }
    };

    debug!("Retrieving existing stacks");
    let mut get_existing_stacks_config = get_stacks_config.clone();
    get_existing_stacks_config
        .filters
        .as_mut()
        .unwrap()
        .push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            8 + 32 + 32 + 8 + 1,
            vec![marketplace::StackStateDiscriminator::Active as u8],
        )));
    let existing_stacks = rpc_client
        .get_program_accounts_with_config(&marketplace::id(), get_existing_stacks_config)
        .await
        .context("Failed to fetch existing stacks from Solana")?;

    let existing_stacks = existing_stacks
        .into_iter()
        .map(|s| {
            let stack = read_solana_stack_account(s)?;
            match stack {
                StackWithState::Active(s) => Ok(s),
                StackWithState::Deleted { .. } => bail!("Got deleted stack on startup"),
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to parse stacks retrieved from Solana")?;

    info!(
        "Received {} existing stacks from Solana",
        existing_stacks.len()
    );

    debug!("Retrieving existing request signers");
    let mut get_existing_request_signers_config = get_request_signers_config.clone();
    get_existing_request_signers_config
        .filters
        .as_mut()
        .unwrap()
        .push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            8 + 32 + 32 + 32 + 1,
            vec![1u8],
        )));
    let existing_request_signers = rpc_client
        .get_program_accounts_with_config(&marketplace::id(), get_existing_request_signers_config)
        .await
        .context("Failed to fetch existing request_signers from Solana")?
        .into_iter()
        .map(|r| {
            let request_signer = read_solana_request_signer_account(r.1)?;
            if request_signer.active {
                Ok(request_signer)
            } else {
                bail!("Got inactive request signer at startup")
            }
        })
        .collect::<Result<Vec<_>>>()
        .context("Failed to parse request signers retrieved from Solana")?
        .into_iter()
        .map(|r| {
            (
                ApiRequestSigner::Solana(r.signer),
                StackOwner::Solana(r.user.to_bytes()),
            )
        })
        .collect::<Vec<_>>();

    let (solana_provider_pda, _) = Pubkey::find_program_address(
        &[
            b"provider",
            &config.solana_provider_public_key.public_key.to_bytes(),
        ],
        &marketplace::id(),
    );

    debug!("Fetching developer escrow balances");
    let owner_states = get_owner_states(
        &rpc_client,
        &solana_provider_pda,
        existing_stacks,
        region.min_escrow_balance,
    )
    .await?;
    let escrow_balances = owner_states
        .iter()
        .map(|(k, v)| (Pubkey::new_from_array(k.to_inner()), v.1))
        .collect();
    let stacks =
        StackCollection::from_known(owner_states.into_iter().map(|(k, v)| (k, (v.0, v.2))));

    debug!("Setting up escrow subscriptions");
    solana_pub_sub.escrow_subscriptions = setup_solana_escrow_subscriptions(
        unsafe {
            (solana_pub_sub.client_wrapper.deref() as *const SolanaPubSubClientWrapper).as_ref()
        }
        .unwrap(),
        &solana_provider_pda,
        stacks.owners().map(|o| match o {
            StackOwner::Solana(pk) => Pubkey::new_from_array(*pk),
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
            min_escrow_balance: region.min_escrow_balance,
            escrow_balances,
        },
        usage_aggregator,
    };

    let tick_interval = *config.solana_usage_report_interval;

    notification_channel.send(BlockchainMonitorNotification::RequestSignersAvailable(
        existing_request_signers,
    ));

    let mailbox = PlainMailboxProcessor::start(
        |_mailbox, message_receiver| {
            mailbox_body(config, state, message_receiver, notification_channel)
        },
        10000,
    );

    let res = BlockchainMonitorImpl { mailbox };

    let res_clone = res.clone();
    tokio::spawn(async move { generate_tick(res_clone, tick_interval).await });

    let region_config = RegionConfig {
        id: region_pda.to_bytes().into(),
        max_giga_instructions_per_call: Some(region.max_giga_instructions_per_call),
    };

    debug!("Initialization complete");
    Ok((Box::new(res), rx, region_config))
}

async fn get_owner_states(
    rpc_client: &RpcClient,
    provider_pda: &Pubkey,
    stacks: impl IntoIterator<Item = StackWithMetadata>,
    min_escrow_balance: u64,
) -> Result<HashMap<StackOwner, (OwnerState, u64, Vec<StackWithMetadata>)>> {
    let by_owner = stacks.into_iter().group_by(|s| s.owner());

    let mut res = HashMap::new();

    for (owner, stacks) in &by_owner {
        let escrow_balance = fetch_owner_escrow_balance(rpc_client, &owner, provider_pda)
            .await?
            .unwrap_or(0);

        let state = if escrow_balance >= min_escrow_balance {
            OwnerState::Active
        } else {
            OwnerState::Inactive
        };

        trace!("Developer {owner:?} has escrow balance {escrow_balance} and state {state:?}");
        res.insert(owner, (state, escrow_balance, stacks.collect()));
    }

    Ok(res)
}

async fn fetch_owner_escrow_balance(
    rpc_client: &RpcClient,
    owner: &StackOwner,
    provider_pda: &Pubkey,
) -> Result<Option<u64>> {
    let StackOwner::Solana(owner_key) = owner;
    //b"escrow", user.key().as_ref(), provider.key().as_ref()
    let (escrow_pda, _) = Pubkey::find_program_address(
        &[b"escrow", owner_key, &provider_pda.to_bytes()],
        &marketplace::id(),
    );

    let token_balance = match rpc_client.get_token_account_balance(&escrow_pda).await {
        Ok(x) => Some(
            x.amount
                .parse()
                .context("Failed to parse amount from token account")?,
        ),
        Err(ClientError {
            // -32602 is "could not find account"
            kind: ClientErrorKind::RpcError(RpcError::RpcResponseError { code: -32602, .. }),
            ..
        }) => None,
        Err(f) => return Err(f).context("Failed to fetch escrow balance from Solana"),
    };

    Ok(token_balance)
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

                    Some(BlockchainMonitorMessage::GetEscrowBalance(owner, r)) => {
                        let pubkey = Pubkey::new_from_array(owner.to_inner());
                        let mut balance = state.solana.escrow_balances.get(&pubkey).copied();
                        if balance.is_none() {
                            match fetch_owner_escrow_balance(&state.solana.rpc_client, &owner, &state.solana.provider_pda).await {
                                Ok(x) => balance = x,
                                Err(f) => {
                                    warn!("Failed to fetch escrow balance for {pubkey} because {f:?}");
                                }
                            }
                        }

                        r.reply(
                            balance.map(|b| EscrowBalance {
                                user_balance:
                                    token_amount_to_ui_amount(
                                        b,
                                        state.solana.token_decimals
                                    )
                                    .ui_amount
                                    .unwrap(),
                                min_balance:
                                    token_amount_to_ui_amount(
                                        state.solana.min_escrow_balance,
                                        state.solana.token_decimals
                                    )
                                    .ui_amount
                                    .unwrap(),
                            })
                        )
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
                        stack,
                        &notification_channel
                    ).await {
                        warn!("Failed to process new stack: {f}");
                    }
                } else {
                    warn!("Solana notification stream disconnected, attempting to reconnect");
                    // TODO: this will make the mailbox stop processing messages while waiting to reconnect
                    // should probably handle subscriptions on a separate task
                    state = reconnect_solana_subscriber(state, &config).await;
                }
            }

            request_signer = state.solana.pub_sub.request_signer_subscription.stream.next() => {
                if let Some(request_signer) = request_signer {
                    debug!("Received request signer update");
                    if let Err(f) = on_request_signer_received(
                        request_signer,
                        &notification_channel
                    ) {
                        warn!("Failed to process request signer: {f}");
                    }
                } else {
                    warn!("Solana notification stream disconnected, attempting to reconnect");
                    // TODO: this will make the mailbox stop processing messages while waiting to reconnect
                    // should probably handle subscriptions on a separate task
                    state = reconnect_solana_subscriber(state, &config).await;
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
                        state = reconnect_solana_subscriber(state, &config).await;
                    },
                    Ok(Some((owner_pubkey, escrow_balance))) =>
                        on_solana_escrow_updated(
                            &mut state,
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
    (state
        .solana
        .pub_sub
        .request_signer_subscription
        .unsubscribe_callback)()
    .await;

    if let Some(r) = stop_reply_channel {
        r.reply(());
    }
}

async fn select_next_escrow_update(
    subs: &mut HashMap<Pubkey, SolanaSubscription<'_, UiAccount>>,
    token_decimals: u8,
) -> Result<Option<(Pubkey, u64)>> {
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
) -> Result<Option<(Pubkey, u64)>> {
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
            let amount = account
                .token_amount
                .amount
                .parse()
                .context("Failed to parse amount of token account, this should never happen")?;
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
    notification_channel: &NotificationChannel<BlockchainMonitorNotification>,
    owner_pubkey: Pubkey,
    escrow_balance: u64,
) {
    state
        .solana
        .escrow_balances
        .insert(owner_pubkey, escrow_balance);

    let new_state = if escrow_balance >= state.solana.min_escrow_balance {
        OwnerState::Active
    } else {
        OwnerState::Inactive
    };

    trace!("Developer {owner_pubkey} should be in state {new_state:?} due to escrow update");

    let owner = StackOwner::Solana(owner_pubkey.to_bytes());
    let owner_entry = state.stacks.owner_entry(owner);
    match owner_entry {
        OwnerEntry::Vacant(_) => {
            warn!("Received escrow update for unknown developer {owner_pubkey}");
        }

        OwnerEntry::Occupied(occ) => {
            let old_state = occ.owner_state();
            if old_state != new_state {
                match new_state {
                    OwnerState::Active => {
                        trace!(
                            "Transitioning {owner_pubkey} to active state, \
                            stacks will be deployed for this owner"
                        );
                        let stacks = occ.stacks().cloned().collect::<Vec<_>>();
                        state.stacks.make_active(&owner);
                        notification_channel
                            .send(BlockchainMonitorNotification::StacksAvailable(stacks));
                    }

                    OwnerState::Inactive => {
                        trace!(
                            "Transitioning {owner_pubkey} to inactive state, \
                            stacks will be undeployed for this owner"
                        );
                        let stack_id_modes = occ
                            .stacks()
                            .map(|s| (s.id(), StackRemovalMode::Temporary))
                            .collect::<Vec<_>>();
                        state.stacks.make_inactive(&owner);
                        notification_channel
                            .send(BlockchainMonitorNotification::StacksRemoved(stack_id_modes));
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
    let rpc_url = config.solana_cluster_rpc_url.0.to_string();
    let pub_sub_url = config.solana_cluster_pub_sub_url.0.to_string();
    let signer_private_key = Keypair::from_bytes(
        config
            .solana_usage_signer_private_key
            .keypair
            .to_bytes()
            .as_slice(),
    )
    .unwrap();
    let commission_pda = Pubkey::find_program_address(&[b"commission"], &marketplace::id()).0;

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
                commission_pda,
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
    commission_account: Pubkey,
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
        commission_token: commission_account,
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

pub struct RegionConfig {
    pub id: Vec<u8>,
    pub max_giga_instructions_per_call: Option<u32>,
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
async fn reconnect_solana_subscriber<'a>(
    state: State<'a>,
    config: &BlockchainMonitorConfig,
) -> State<'a> {
    debug!("Reconnecting solana subscriptions");

    (state.solana.pub_sub.stack_subscription.unsubscribe_callback)().await;
    (state
        .solana
        .pub_sub
        .request_signer_subscription
        .unsubscribe_callback)()
    .await;

    let client_wrapper = loop {
        match PubsubClient::new(&config.solana_cluster_pub_sub_url.0.to_string())
            .await
            .context("Failed to start Solana pub-sub client")
        {
            Ok(client) => {
                break Box::pin(SolanaPubSubClientWrapper {
                    client,
                    _phantom_pinned: PhantomPinned,
                })
            }

            Err(f) => {
                warn!("{f:?}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    };

    // TODO: this will miss stacks deployed between when we were disconnected and when we managed to connect back.
    let wrapper_ref: &SolanaPubSubClientWrapper =
        unsafe { (client_wrapper.deref() as *const SolanaPubSubClientWrapper).as_ref() }.unwrap();
    let (stack_subscription, request_signer_subscription, escrow_subscriptions) =
        setup_solana_subscriptions(
            wrapper_ref,
            &state.solana.pub_sub.get_stacks_config,
            &state.solana.pub_sub.get_request_signers_config,
            &state.solana.provider_pda,
            state.stacks.owners().map(|o| match o {
                StackOwner::Solana(pk) => Pubkey::new_from_array(*pk),
            }),
        )
        .await;

    info!("Reconnected to Solana");

    State {
        solana: Solana {
            pub_sub: SolanaPubSub {
                stack_subscription,
                request_signer_subscription,
                escrow_subscriptions,
                client_wrapper,
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
    get_request_signers_config: &RpcProgramAccountsConfig,
    provider_pda: &Pubkey,
    owners: impl Iterator<Item = Pubkey> + Clone,
) -> (
    SolanaSubscription<'a, RpcKeyedAccount>,
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
                warn!("{f:?}");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let request_signer_subscription = match pub_sub_client_wrapper
            .client
            .program_subscribe(&marketplace::id(), Some(get_request_signers_config.clone()))
            .await
            .context("Failed to re-setup Solana subscription for new request signers")
        {
            Ok((stream, unsubscribe_callback)) => SolanaSubscription {
                stream,
                unsubscribe_callback,
            },
            Err(f) => {
                warn!("{f:?}");
                tokio::time::sleep(Duration::from_secs(1)).await;
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
                warn!("{f:?}");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        return (
            stack_subscription,
            request_signer_subscription,
            escrow_subscriptions,
        );
    }
}

async fn setup_solana_escrow_subscriptions<'a>(
    pub_sub_client_wrapper: &'a SolanaPubSubClientWrapper,
    provider_pda: &Pubkey,
    owners: impl Iterator<Item = Pubkey>,
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
            owner_id,
            SolanaSubscription {
                stream,
                unsubscribe_callback,
            },
        );
    }

    Ok(escrow_subscriptions)
}

fn on_request_signer_received(
    account: Response<RpcKeyedAccount>,
    notification_channel: &NotificationChannel<BlockchainMonitorNotification>,
) -> Result<()> {
    let account: Account = account
        .value
        .account
        .decode()
        .ok_or_else(|| anyhow!("Failed to decode request signer Account"))?;

    let request_signer_account = read_solana_request_signer_account(account)?;

    if request_signer_account.active {
        notification_channel.send(BlockchainMonitorNotification::RequestSignersAvailable(
            vec![(
                ApiRequestSigner::Solana(request_signer_account.signer),
                StackOwner::Solana(request_signer_account.user.to_bytes()),
            )],
        ));
    } else {
        notification_channel.send(BlockchainMonitorNotification::RequestSignersRemoved(vec![
            ApiRequestSigner::Solana(request_signer_account.signer),
        ]));
    }

    Ok(())
}

fn read_solana_request_signer_account(account: Account) -> Result<marketplace::ApiRequestSigner> {
    marketplace::ApiRequestSigner::try_deserialize(&mut &account.data[..])
        .context("Failed to deserialize request signer data")
}

async fn on_new_stack_received(
    state: &mut State<'_>,
    account: Response<RpcKeyedAccount>,
    notification_channel: &NotificationChannel<BlockchainMonitorNotification>,
) -> Result<()> {
    let stack = read_solana_rpc_keyed_account(account)
        .context("Received stack from blockchain but failed to deserialize")?;

    match stack {
        StackWithState::Active(stack) => {
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
                    trace!("This stack is from a new owner, fetching escrow balance");
                    let escrow_balance = fetch_owner_escrow_balance(
                        &state.solana.rpc_client,
                        &stack.owner(),
                        &state.solana.provider_pda,
                    )
                    .await?
                    .unwrap_or(0);
                    let state = if escrow_balance >= state.solana.min_escrow_balance {
                        OwnerState::Active
                    } else {
                        OwnerState::Inactive
                    };

                    trace!("New owner has escrow balance {escrow_balance}, adding with state {state:?}");

                    vac.insert_first(state, stack.clone());
                    true
                }
            };

            if should_report_stack {
                notification_channel
                    .send(BlockchainMonitorNotification::StacksAvailable(vec![stack]));
            }
        }

        StackWithState::Deleted { stack_id, owner_id } => {
            debug!(
                "Received deletion notification for stack with ID {:?}",
                stack_id
            );

            if let OwnerEntry::Occupied(occ) = state.stacks.owner_entry(owner_id) {
                if occ.remove_stack(stack_id).0 {
                    notification_channel.send(BlockchainMonitorNotification::StacksRemoved(vec![
                        (stack_id, StackRemovalMode::Permanent),
                    ]));
                }
            }
        }
    }

    Ok(())
}

fn read_solana_stack_account((pubkey, account): (Pubkey, Account)) -> Result<StackWithState> {
    let stack_account = marketplace::Stack::try_deserialize(&mut &account.data[..])
        .context("Failed to deserialize Stack data")?;

    match stack_account.state {
        marketplace::StackState::Active {
            revision,
            name,
            stack_data,
        } => {
            let stack_definition = mu_stack::Stack::try_deserialize_proto(stack_data)
                .context("Failed to deserialize stack definition")?;

            let validated_stack = stack_definition
                .validate()
                .map_err(|(_, e)| e)
                .context("Invalid stack definition")?;

            Ok(StackWithState::Active(StackWithMetadata {
                stack: validated_stack,
                name,
                revision,
                metadata: StackMetadata::Solana(super::SolanaStackMetadata {
                    account_id: pubkey,
                    owner: stack_account.user,
                }),
            }))
        }

        marketplace::StackState::Deleted => Ok(StackWithState::Deleted {
            stack_id: StackID::SolanaPublicKey(pubkey.to_bytes()),
            owner_id: StackOwner::Solana(stack_account.user.to_bytes()),
        }),
    }
}

fn read_solana_rpc_keyed_account(stack: Response<RpcKeyedAccount>) -> Result<StackWithState> {
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
    read_solana_stack_account((pubkey, account))
}

// TODO: ensure we have the right usage signer for this region
async fn get_region(
    region: &Pubkey,
    rpc_client: &RpcClient,
) -> Result<marketplace::ProviderRegion> {
    debug!("Fetching region data from Solana");

    let account = rpc_client.get_account(region).await.context(format!(
        "Failed to fetch region {region} from Solana, make sure the `solana_provider_public_key` and \
            `solana_region_num` config values are correct, the region is already created, the URL in \
            `solana_cluster_rpc_url` is available and that you are not running into rate limits",
    ))?;

    let region =
        marketplace::ProviderRegion::try_deserialize(&mut &account.data[..]).context(format!(
            "Failed to deserialize region in {region}, ensure the region was deployed correctly"
        ))?;
    info!("Starting in region {}", region.name);

    Ok(region)
}
