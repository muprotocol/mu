use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use mu::stack::usage_aggregator::{UsageAggregator, UsageCategory};
use mu_stack::StackID;

use anyhow::Result;

#[derive(Clone)]
pub struct HashMapUsageAggregator {
    inner: Arc<Mutex<HashMap<StackID, HashMap<UsageCategory, u128>>>>,
}

impl HashMapUsageAggregator {
    pub fn new() -> Box<dyn UsageAggregator> {
        Box::new(Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

#[async_trait]
impl UsageAggregator for HashMapUsageAggregator {
    fn register_usage(
        &self,
        stack_id: mu_stack::StackID,
        usage: Vec<mu::stack::usage_aggregator::Usage>,
    ) {
        let mut map = self.inner.lock().unwrap();
        let stack_usage_map = map.entry(stack_id).or_insert_with(HashMap::new);

        for usage in usage {
            let (category, amount) = usage.into_category();
            let usage_amount = stack_usage_map.entry(category).or_insert(0);
            *usage_amount += amount;
        }
    }

    async fn get_and_reset_usages(
        &self,
    ) -> Result<HashMap<mu_stack::StackID, HashMap<mu::stack::usage_aggregator::UsageCategory, u128>>>
    {
        let mut map = self.inner.lock().unwrap();
        let usages = map.drain().collect();
        Ok(usages)
    }

    async fn stop(&self) {}
}
