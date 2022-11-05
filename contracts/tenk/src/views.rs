use crate::*;

#[near_bindgen]
impl Contract {
    /// Current contract owner
    pub fn owner(&self) -> AccountId {
        self.tokens.owner_id.clone()
    }

    /// Part of the NFT metadata standard. Returns the contract's metadata
    pub fn contract_metadata(&self) -> NFTContractMetadata {
        self.metadata.get().unwrap()
    }
}
