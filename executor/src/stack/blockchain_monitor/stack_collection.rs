use crate::stack::{StackOwner, StackWithMetadata};
use mu_stack::StackID;
use std::collections::{hash_map, HashMap, HashSet};

pub enum StackState {
    Active(StackWithMetadata),
    Inactive(StackWithMetadata),
}

impl StackState {
    fn stack(&self) -> &StackWithMetadata {
        match self {
            Self::Active(s) => s,
            Self::Inactive(s) => s,
        }
    }
}

#[derive(Default)]
pub(super) struct StackCollection {
    stacks: HashMap<StackID, StackState>,
    stacks_by_user: HashMap<StackOwner, HashSet<StackID>>,
    active_stacks: HashSet<StackID>,
    inactive_stacks: HashSet<StackID>,
}

impl StackCollection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_known(known_stacks: impl Iterator<Item = StackState>) -> Self {
        let mut result = Self::default();
        for stack in known_stacks {
            result.insert(stack);
        }
        result
    }

    pub fn insert_active(&mut self, stack: StackWithMetadata) -> bool {
        self.insert(StackState::Active(stack))
    }

    pub fn insert_inactive(&mut self, stack: StackWithMetadata) -> bool {
        self.insert(StackState::Inactive(stack))
    }

    pub fn insert(&mut self, stack_state: StackState) -> bool {
        let stack = stack_state.stack();
        let stack_id = stack.id();
        let owner = stack.owner();

        if self.stacks.contains_key(&stack_id) {
            return false;
        }

        match stack_state {
            StackState::Active(_) => &mut self.active_stacks,
            StackState::Inactive(_) => &mut self.inactive_stacks,
        }
        .insert(stack_id);

        self.stacks.insert(stack_id, stack_state);

        self.stacks_by_user
            .entry(owner)
            .or_insert_with(HashSet::new)
            .insert(stack_id);

        true
    }

    pub fn remove(&mut self, stack_id: &StackID) {
        if let Some(stack) = self.stacks.remove(stack_id) {
            self.active_stacks.remove(stack_id);
            self.inactive_stacks.remove(stack_id);
            let owner = stack.stack().owner();
            if let Some(set) = self.stacks_by_user.get_mut(&owner) {
                set.remove(stack_id);
                if set.is_empty() {
                    self.stacks_by_user.remove(&owner);
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stacks.is_empty()
    }

    pub fn owners(&self) -> impl Iterator<Item = &StackOwner> + Clone {
        self.stacks_by_user.keys()
    }

    pub fn all(&self) -> impl Iterator<Item = &StackState> {
        self.stacks.values()
    }

    pub fn all_active(&self) -> impl Iterator<Item = &StackWithMetadata> {
        self.active_stacks
            .iter()
            .map(|id| match self.stacks.get(id) {
                Some(StackState::Active(stack)) => stack,
                Some(StackState::Inactive(_)) => panic!(
                    "Internal indexing error: expected stack {id} to be active, but it's inactive"
                ),
                None => panic!(
                    "Internal indexing error: expected stack {id} to be active, but it is unknown"
                ),
            })
    }

    pub fn all_inactive(&self) -> impl Iterator<Item = &StackWithMetadata> {
        self.inactive_stacks
            .iter()
            .map(|id| match self.stacks.get(id) {
                Some(StackState::Inactive(stack)) => stack,
                Some(StackState::Active(_)) => panic!(
                    "Internal indexing error: expected stack {id} to be inactive, but it's active"
                ),
                None => panic!(
                    "Internal indexing error: expected stack {id} to be active, but it is unknown"
                ),
            })
    }

    pub fn entry(&mut self, stack_id: StackID) -> Entry {
        let indices = Indices {
            stacks_by_user: &mut self.stacks_by_user,
            active_stacks: &mut self.active_stacks,
            inactive_stacks: &mut self.inactive_stacks,
        };

        match self.stacks.entry(stack_id) {
            hash_map::Entry::Vacant(v) => Entry::Vacant(VacantEntry(indices, v)),
            hash_map::Entry::Occupied(occ) => match occ.get() {
                StackState::Active(_) => Entry::Active(ActiveEntry(indices, occ)),
                StackState::Inactive(_) => Entry::Inactive(InactiveEntry(indices, occ)),
            },
        }
    }
}

struct Indices<'a> {
    stacks_by_user: &'a mut HashMap<StackOwner, HashSet<StackID>>,
    active_stacks: &'a mut HashSet<StackID>,
    inactive_stacks: &'a mut HashSet<StackID>,
}

impl<'a> Indices<'a> {
    fn insert(&mut self, stack_id: StackID, owner: StackOwner, active: bool) {
        if active {
            &mut self.active_stacks
        } else {
            &mut self.inactive_stacks
        }
        .insert(stack_id);

        self.stacks_by_user
            .entry(owner)
            .or_insert_with(HashSet::new)
            .insert(stack_id);
    }

    fn remove(&mut self, stack_id: StackID, owner: StackOwner) {
        self.active_stacks.remove(&stack_id);
        self.inactive_stacks.remove(&stack_id);
        if let Some(set) = self.stacks_by_user.get_mut(&owner) {
            set.remove(&stack_id);
            if set.is_empty() {
                self.stacks_by_user.remove(&owner);
            }
        }
    }
}

pub(super) enum Entry<'a> {
    Vacant(VacantEntry<'a>),
    Active(ActiveEntry<'a>),
    Inactive(InactiveEntry<'a>),
}

pub(super) struct VacantEntry<'a>(Indices<'a>, hash_map::VacantEntry<'a, StackID, StackState>);

pub(super) struct ActiveEntry<'a>(
    Indices<'a>,
    hash_map::OccupiedEntry<'a, StackID, StackState>,
);

pub(super) struct InactiveEntry<'a>(
    Indices<'a>,
    hash_map::OccupiedEntry<'a, StackID, StackState>,
);

impl<'a> VacantEntry<'a> {
    pub fn insert_active(mut self, stack: StackWithMetadata) {
        self.0.insert(stack.id(), stack.owner(), true);
        self.1.insert(StackState::Active(stack));
    }

    pub fn insert_inactive(mut self, stack: StackWithMetadata) {
        self.0.insert(stack.id(), stack.owner(), false);
        self.1.insert(StackState::Inactive(stack));
    }
}

impl<'a> ActiveEntry<'a> {
    pub fn make_inactive(mut self) {
        let stack = self.1.get_mut();
        let id = stack.stack().id();

        // Still waiting for that hash_map::Entry::replace_with API
        *stack = StackState::Inactive(stack.stack().clone());
        self.0.active_stacks.remove(&id);
        self.0.inactive_stacks.insert(id);
    }

    pub fn get(&self) -> &StackWithMetadata {
        self.1.get().stack()
    }

    pub fn remove(mut self) {
        let stack = self.1.remove();
        let stack = stack.stack();
        self.0.remove(stack.id(), stack.owner());
    }
}

impl<'a> InactiveEntry<'a> {
    pub fn make_active(mut self) {
        let stack = self.1.get_mut();
        let id = stack.stack().id();

        // Still waiting for that hash_map::Entry::replace_with API
        *stack = StackState::Active(stack.stack().clone());
        self.0.inactive_stacks.remove(&id);
        self.0.active_stacks.insert(id);
    }

    pub fn get(&self) -> &StackWithMetadata {
        self.1.get().stack()
    }

    pub fn remove(mut self) {
        let stack = self.1.remove();
        let stack = stack.stack();
        self.0.remove(stack.id(), stack.owner());
    }
}
