// We don't use all of this yet, but I expect it will come in handy later.
#![allow(dead_code)]

use std::collections::{hash_map, HashMap, HashSet};
use std::default::Default;

use mu_stack::{StackID, StackOwner};

use crate::stack::StackWithMetadata;

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum OwnerState {
    Active,
    Inactive,
}

struct OwnerData {
    state: OwnerState,
    stacks: HashSet<StackID>,
}

impl OwnerData {
    fn new(state: OwnerState) -> Self {
        Self {
            state,
            stacks: HashSet::new(),
        }
    }
}

#[derive(Default)]
pub(super) struct StackCollection {
    stacks: HashMap<StackID, StackWithMetadata>,
    owners: HashMap<StackOwner, OwnerData>,
    active_owners: HashSet<StackOwner>,
    inactive_owners: HashSet<StackOwner>,
}

impl StackCollection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_known(
        known_stacks: HashMap<StackOwner, (OwnerState, Vec<StackWithMetadata>)>,
    ) -> Self {
        let mut result = Self::default();
        for (owner, (state, stacks)) in known_stacks {
            let mut owner_data = OwnerData::new(state);
            for stack in stacks {
                let id = stack.id();
                if result.stacks.insert(id, stack).is_some() {
                    panic!("Duplicate stack ID {id}");
                }
                owner_data.stacks.insert(id);
            }

            result.owners.insert(owner.clone(), owner_data);

            let index = result.get_owner_index(state);
            index.insert(owner);
        }
        result
    }

    fn get_owner_index(&mut self, state: OwnerState) -> &mut HashSet<StackOwner> {
        match state {
            OwnerState::Active => &mut self.active_owners,
            OwnerState::Inactive => &mut self.inactive_owners,
        }
    }

    pub fn make_inactive(&mut self, owner: &StackOwner) {
        if let Some(owner_data) = self.owners.get_mut(owner) {
            if let OwnerState::Active = owner_data.state {
                owner_data.state = OwnerState::Inactive;
                self.active_owners.remove(owner);
                self.inactive_owners.insert(owner.clone());
            }
        }
    }

    pub fn make_active(&mut self, owner: &StackOwner) {
        if let Some(owner_data) = self.owners.get_mut(owner) {
            if let OwnerState::Inactive = owner_data.state {
                owner_data.state = OwnerState::Active;
                self.inactive_owners.remove(owner);
                self.active_owners.insert(owner.clone());
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stacks.is_empty()
    }

    pub fn owners(&self) -> impl Iterator<Item = &StackOwner> + Clone {
        self.owners.keys()
    }

    pub fn all(&self) -> impl Iterator<Item = &StackWithMetadata> {
        self.stacks.values()
    }

    pub fn all_active(&self) -> impl Iterator<Item = &StackWithMetadata> {
        self.all_by_owners(self.active_owners.iter())
    }

    pub fn all_inactive(&self) -> impl Iterator<Item = &StackWithMetadata> {
        self.all_by_owners(self.inactive_owners.iter())
    }

    fn all_by_owners<'a>(
        &'a self,
        owners: impl IntoIterator<Item = &'a StackOwner>,
    ) -> impl Iterator<Item = &'a StackWithMetadata> {
        owners.into_iter().flat_map(|owner| {
            self.owners
                .get(owner)
                .expect("Owner indexes are out of sync; expected to know owner")
                .stacks
                .iter()
                .map(|id| {
                    self.stacks
                        .get(id)
                        .expect("Stack index is out of sync, expected to know stack")
                })
        })
    }

    pub fn entry(&mut self, stack_id: StackID) -> Entry {
        match self.stacks.entry(stack_id) {
            hash_map::Entry::Vacant(_) => Entry::Vacant,
            hash_map::Entry::Occupied(occ) => {
                match self.owners.get(&occ.get().owner()).unwrap().state {
                    OwnerState::Active => Entry::Active(ActiveEntry(occ)),
                    OwnerState::Inactive => Entry::Inactive(InactiveEntry(occ)),
                }
            }
        }
    }

    pub fn owner_entry(&mut self, owner: StackOwner) -> OwnerEntry {
        match self.owners.get(&owner) {
            Some(_) => OwnerEntry::Occupied(OccupiedOwnerEntry(self, owner)),
            None => OwnerEntry::Vacant(VacantOwnerEntry(self, owner)),
        }
    }
}

pub(super) enum Entry<'a> {
    Vacant,
    Active(ActiveEntry<'a>),
    Inactive(InactiveEntry<'a>),
}

pub(super) struct ActiveEntry<'a>(hash_map::OccupiedEntry<'a, StackID, StackWithMetadata>);

pub(super) struct InactiveEntry<'a>(hash_map::OccupiedEntry<'a, StackID, StackWithMetadata>);

impl<'a> ActiveEntry<'a> {
    pub fn get(&self) -> &StackWithMetadata {
        self.0.get()
    }
}

impl<'a> InactiveEntry<'a> {
    pub fn get(&self) -> &StackWithMetadata {
        self.0.get()
    }
}

pub(super) enum OwnerEntry<'a> {
    Vacant(VacantOwnerEntry<'a>),
    Occupied(OccupiedOwnerEntry<'a>),
}

pub(super) struct VacantOwnerEntry<'a>(&'a mut StackCollection, StackOwner);

pub(super) struct OccupiedOwnerEntry<'a>(&'a mut StackCollection, StackOwner);

impl<'a> VacantOwnerEntry<'a> {
    pub(super) fn insert_first(
        self,
        state: OwnerState,
        stack: StackWithMetadata,
    ) -> OccupiedOwnerEntry<'a> {
        self.0.owners.insert(self.1.clone(), OwnerData::new(state));
        self.0.get_owner_index(state).insert(self.1.clone());
        let mut result = OccupiedOwnerEntry(self.0, self.1);
        result.add_stack(stack);
        result
    }
}

impl<'a> OccupiedOwnerEntry<'a> {
    /// Returns true if the stack was previously unknown *or* it's a newer version
    /// of a previously known stack.
    pub(super) fn add_stack(&mut self, stack: StackWithMetadata) -> bool {
        let stack_id = stack.id();

        if stack.owner() != self.1 {
            panic!(
                "Stack {stack_id} belongs to {:?} not {:?}",
                stack.owner(),
                self.1
            );
        }

        if self.0.stacks.contains_key(&stack_id) {
            let existing = self.0.stacks.get(&stack_id).unwrap();
            if existing.revision >= stack.revision {
                return false;
            }

            self.0.stacks.insert(stack_id, stack);
            true
        } else {
            let owner_data = self.0.owners.get_mut(&self.1).unwrap();
            self.0.stacks.insert(stack_id, stack);
            owner_data.stacks.insert(stack_id);

            true
        }
    }

    // We remove any and all info about owners once all their stacks are removed.
    // This seems like a logical thing to do, since they probably won't be coming
    // back, and we don't want to waste precious RAM tracking them.
    pub(super) fn remove_stack(self, stack_id: StackID) -> (bool, OwnerEntry<'a>) {
        let owner_data = self.0.owners.get_mut(&self.1).unwrap();
        if !owner_data.stacks.remove(&stack_id) {
            return (false, OwnerEntry::Occupied(self));
        }

        self.0.stacks.remove(&stack_id);

        if owner_data.stacks.is_empty() {
            self.0.owners.remove(&self.1);
            (true, OwnerEntry::Vacant(VacantOwnerEntry(self.0, self.1)))
        } else {
            (
                false,
                OwnerEntry::Occupied(OccupiedOwnerEntry(self.0, self.1)),
            )
        }
    }

    pub(super) fn owner_state(&self) -> OwnerState {
        self.0.owners.get(&self.1).unwrap().state
    }

    pub(super) fn stacks(&self) -> impl Iterator<Item = &StackWithMetadata> {
        self.0.owners.get(&self.1).unwrap().stacks.iter().map(|id| {
            self.0
                .stacks
                .get(id)
                .expect("owners.stacks is out of sync with stacks")
        })
    }
}
