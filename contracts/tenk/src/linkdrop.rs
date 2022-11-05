use crate::*;
use near_sdk::{
    env, ext_contract, json_types::U128, log, near_bindgen, AccountId, Balance, Gas, Promise,
    PublicKey,
};
use near_units::parse_near;

/// 0.064311394105062020653824 N
pub(crate) const ACCESS_KEY_ALLOWANCE: u128 = parse_near!("0 N");

pub(crate) const LINKDROP_DEPOSIT: u128 = parse_near!("0.02 N");
/// can take 0.5 of access key since gas required is 6.6 times what was actually used
const ON_CREATE_ACCOUNT_GAS: Gas = Gas(30_000_000_000_000);
const NO_DEPOSIT: Balance = 0;

/// Gas attached to the callback from account creation.
pub const ON_CREATE_ACCOUNT_CALLBACK_GAS: Gas = Gas(10_000_000_000_000);

#[ext_contract(ext_linkdrop)]
trait ExtLinkdrop {
    fn create_account(&mut self, new_account_id: AccountId, new_public_key: PublicKey) -> Promise;
    fn on_create_and_claim(&mut self, mint_for_free: bool) -> bool;
}