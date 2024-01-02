use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use dyn_clonable::clonable;
use mailbox_processor::{callback::CallbackMailboxProcessor, ReplyChannel};
use mu_stack::StackID;

use super::{ApiRequestSigner, StackOwner};

#[async_trait]
#[clonable]
pub trait RequestSignerCache: Clone + Send + Sync {
    async fn validate_signer(&self, stack_id: StackID, signer: ApiRequestSigner) -> Result<bool>;

    async fn stacks_available(&self, stacks: Vec<(StackID, StackOwner)>) -> Result<()>;
    async fn stacks_removed(&self, stack_ids: Vec<StackID>) -> Result<()>;
    async fn signers_available(&self, signers: Vec<(ApiRequestSigner, StackOwner)>) -> Result<()>;
    async fn signers_removed(&self, signers: Vec<ApiRequestSigner>) -> Result<()>;

    async fn stop(&self);
}

struct State {
    stacks: HashMap<StackID, StackOwner>,
    signers: HashMap<ApiRequestSigner, StackOwner>,
}

enum Message {
    ValidateSigner(StackID, ApiRequestSigner, ReplyChannel<bool>),

    StacksAvailable(Vec<(StackID, StackOwner)>),
    StacksRemoved(Vec<StackID>),
    SignersAvailable(Vec<(ApiRequestSigner, StackOwner)>),
    SignersRemoved(Vec<ApiRequestSigner>),
}

#[derive(Clone)]
struct RequestSignerCacheImpl {
    mailbox: CallbackMailboxProcessor<Message>,
}

#[async_trait]
impl RequestSignerCache for RequestSignerCacheImpl {
    async fn validate_signer(&self, stack_id: StackID, signer: ApiRequestSigner) -> Result<bool> {
        self.mailbox
            .post_and_reply(|r| Message::ValidateSigner(stack_id, signer, r))
            .await
            .map_err(Into::into)
    }

    async fn stacks_available(&self, stacks: Vec<(StackID, StackOwner)>) -> Result<()> {
        self.mailbox
            .post(Message::StacksAvailable(stacks))
            .await
            .map_err(Into::into)
    }

    async fn stacks_removed(&self, stack_ids: Vec<StackID>) -> Result<()> {
        self.mailbox
            .post(Message::StacksRemoved(stack_ids))
            .await
            .map_err(Into::into)
    }

    async fn signers_available(&self, signers: Vec<(ApiRequestSigner, StackOwner)>) -> Result<()> {
        self.mailbox
            .post(Message::SignersAvailable(signers))
            .await
            .map_err(Into::into)
    }

    async fn signers_removed(&self, signers: Vec<ApiRequestSigner>) -> Result<()> {
        self.mailbox
            .post(Message::SignersRemoved(signers))
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) {
        self.mailbox.clone().stop().await;
    }
}

pub fn start() -> Box<dyn RequestSignerCache> {
    let state = State {
        stacks: Default::default(),
        signers: Default::default(),
    };

    let mailbox = CallbackMailboxProcessor::start(mailbox_step, state, 10000);

    Box::new(RequestSignerCacheImpl { mailbox })
}

async fn mailbox_step(
    _mb: CallbackMailboxProcessor<Message>,
    message: Message,
    mut state: State,
) -> State {
    match message {
        Message::ValidateSigner(stack_id, signer, rep) => {
            rep.reply(is_valid_signer(&state, &stack_id, &signer));
        }

        Message::StacksAvailable(stacks) => {
            for (stack_id, owner) in stacks {
                state.stacks.insert(stack_id, owner);
            }
        }

        Message::StacksRemoved(stack_ids) => {
            for stack_id in stack_ids {
                state.stacks.remove(&stack_id);
            }
        }

        Message::SignersAvailable(signers) => {
            for (signer, owner) in signers {
                state.signers.insert(signer, owner);
            }
        }

        Message::SignersRemoved(signers) => {
            for signer in signers {
                state.signers.remove(&signer);
            }
        }
    }

    state
}

fn is_valid_signer(state: &State, stack_id: &StackID, signer: &ApiRequestSigner) -> bool {
    let Some(stack_owner) = state.stacks.get(stack_id) else {
        return false;
    };

    let StackOwner::PWR(stack_owner_pubkey) = stack_owner;
    let ApiRequestSigner::PWR(signer_pubkey) = signer;

    if *signer_pubkey == *stack_owner_pubkey {
        return true;
    }

    let Some(signer_owner) = state.signers.get(signer) else {
        return false;
    };

    *signer_owner == *stack_owner
}
