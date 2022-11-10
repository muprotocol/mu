// TODO: a single mailbox most likely won't manage to keep up with all the billing
// events generated in the entire system. There are a number of ways to improve on
// this design:
// 1. Use many mailboxes, use some sort of hash of the stack ID to figure out which
//    mailbox to send the usage to.
// 2. Use concurrent data structures to store usages as they happen, directly. I don't
//    like this option because it introduces some manner of lock one way or another.

// TODO: store usage data in persistent storage somewhere (Mu DB itself?)

use anyhow::Result;
use async_trait::async_trait;
use dyn_clonable::clonable;
use mailbox_processor::callback::CallbackMailboxProcessor;
use mailbox_processor::ReplyChannel;
use mu_stack::StackID;
use std::collections::HashMap;

#[async_trait]
#[clonable]
pub trait UsageAggregator: Clone + Sync + Send {
    fn register_usage(&self, stack_id: StackID, usage: Vec<Usage>);
    async fn get_and_reset_usages(&self) -> Result<HashMap<StackID, HashMap<UsageCategory, u128>>>;
    async fn stop(&self);
}

pub enum Usage {
    FunctionMBInstructions {
        memory_megabytes: u64,
        instructions: u64,
    },
    DBStorage {
        size_bytes: u64,
        seconds: u64,
    },
    DBRead {
        weak_reads: u64,
        strong_reads: u64,
    },
    DBWrite {
        weak_writes: u64,
        strong_writes: u64,
    },
    GatewayRequests {
        count: u64,
    },
    GatewayTraffic {
        size_bytes: u64,
    },
}

impl Usage {
    // Note: this may not be the best place for logic as important as
    // calculating usage numbers.
    // Also, we count each strong read/write as two weak reads/writes.
    // We *may* want to make this more configurable.
    fn into_category(self) -> (UsageCategory, u128) {
        match self {
            Usage::FunctionMBInstructions {
                instructions,
                memory_megabytes,
            } => (
                UsageCategory::FunctionMBInstructions,
                memory_megabytes as u128 * instructions as u128,
            ),
            Usage::DBStorage {
                size_bytes,
                seconds,
            } => (
                UsageCategory::DBStorage,
                size_bytes as u128 * seconds as u128,
            ),
            Usage::DBRead {
                weak_reads,
                strong_reads,
            } => (
                UsageCategory::DBReads,
                weak_reads as u128 + strong_reads as u128 * 2,
            ),
            Usage::DBWrite {
                weak_writes,
                strong_writes,
            } => (
                UsageCategory::DBWrites,
                weak_writes as u128 + strong_writes as u128 * 2,
            ),
            Usage::GatewayRequests { count } => (UsageCategory::GatewayRequests, count as u128),
            Usage::GatewayTraffic { size_bytes } => {
                (UsageCategory::GatewayTraffic, size_bytes as u128)
            }
        }
    }
}

// This is different from `Usage` above in that it doesn't contain any data in the cases.
// This is useful for storing usages in a hashset, keyed by category. Also, this is perfect
// for reporting directly to the blockchain.
#[derive(PartialEq, Eq, Hash)]
pub enum UsageCategory {
    FunctionMBInstructions,
    DBStorage,
    DBReads,
    DBWrites,
    GatewayRequests,
    GatewayTraffic,
}

enum Message {
    RegisterUsage(StackID, Vec<Usage>),
    GetAndResetUsages(ReplyChannel<HashMap<StackID, HashMap<UsageCategory, u128>>>),
}

#[derive(Clone)]
struct UsageAggregatorImpl {
    mailbox: CallbackMailboxProcessor<Message>,
}

#[async_trait]
impl UsageAggregator for UsageAggregatorImpl {
    fn register_usage(&self, stack_id: StackID, usage: Vec<Usage>) {
        self.mailbox
            .post_and_forget(Message::RegisterUsage(stack_id, usage));
    }

    async fn get_and_reset_usages(&self) -> Result<HashMap<StackID, HashMap<UsageCategory, u128>>> {
        self.mailbox
            .post_and_reply(Message::GetAndResetUsages)
            .await
            .map_err(Into::into)
    }

    async fn stop(&self) {
        self.mailbox.clone().stop().await;
    }
}

struct State {
    usages: HashMap<StackID, HashMap<UsageCategory, u128>>,
}

pub fn start() -> Box<dyn UsageAggregator> {
    let state = State {
        usages: HashMap::new(),
    };

    let mailbox = CallbackMailboxProcessor::start(mailbox_step, state, 10000);

    Box::new(UsageAggregatorImpl { mailbox })
}

async fn mailbox_step(
    _mb: CallbackMailboxProcessor<Message>,
    msg: Message,
    mut state: State,
) -> State {
    match msg {
        Message::RegisterUsage(stack_id, usage) => {
            let stack_usage_map = state.usages.entry(stack_id).or_insert_with(HashMap::new);

            for usage in usage {
                let (category, amount) = usage.into_category();
                let usage_amount = stack_usage_map.entry(category).or_insert(0);
                *usage_amount += amount;
            }

            state
        }

        Message::GetAndResetUsages(rep) => {
            rep.reply(state.usages);
            State {
                usages: HashMap::new(),
            }
        }
    }
}
