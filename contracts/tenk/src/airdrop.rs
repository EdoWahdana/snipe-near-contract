use crate::*;
use near_sdk::{collections::LazyOption, near_bindgen, IntoStorageKey};
use raffle_collection::RaffleCollection;

#[near_bindgen]
impl Contract {}

pub fn get_raffle_collection<S>(prefix: S) -> LazyOption<RaffleCollection>
where
    S: IntoStorageKey,
{
    LazyOption::new(prefix, None)
}

pub fn initialize_raffle_collection<S>(prefix: S, raffle_prefix: S, length: u32, max_winners: u32)
where
    S: IntoStorageKey,
{
    let mut raffle = get_raffle_collection(prefix);
    require!(raffle.get().is_none(), "Raffle is already initialized");
    let inner_raffle = RaffleCollection::new(raffle_prefix, length, max_winners);
    raffle.set(&inner_raffle);
}
