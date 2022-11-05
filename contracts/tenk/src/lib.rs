use linkdrop::LINKDROP_DEPOSIT;
use near_contract_standards::non_fungible_token::{
  metadata::{NFTContractMetadata, TokenMetadata, NFT_METADATA_SPEC},
  refund_deposit_to_account, NonFungibleToken, Token, TokenId,
};
use near_sdk::{
  borsh::{self, BorshDeserialize, BorshSerialize},
  collections::{LazyOption, LookupMap, UnorderedSet},
  env, ext_contract,
  json_types::{Base64VecU8, U128},
  log, near_bindgen, require,
  serde::{Deserialize, Serialize},
  witgen, AccountId, Balance, BorshStorageKey, Gas, PanicOnDefault, Promise, PromiseOrValue,
  PublicKey,
};
use near_units::{parse_gas, parse_near};
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};

/// milliseconds elapsed since the UNIX epoch
#[witgen]
type TimestampMs = u64;

pub mod linkdrop;
mod owner;
pub mod payout;
mod raffle;
mod standards;
mod types;
mod util;
mod views;

use payout::*;
use raffle::Raffle;
use standards::*;
use types::*;
use util::{current_time_ms, is_promise_success, log_mint, refund};

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
  pub(crate) tokens: NonFungibleToken,
  metadata: LazyOption<NFTContractMetadata>,
  /// Vector of available NFTs
  raffle: Raffle,
  pending_tokens: u32,
  /// Linkdrop fields will be removed once proxy contract is deployed
  pub accounts: LookupMap<PublicKey, bool>,
  /// Whitelist
  whitelist: LookupMap<AccountId, Allowance>,

  sale: Sale,

  admins: UnorderedSet<AccountId>,

  /// extension for generating media links
  media_extension: Option<String>,
}

const GAS_REQUIRED_FOR_LINKDROP: Gas = Gas(parse_gas!("40 Tgas") as u64);
const GAS_REQUIRED_TO_CREATE_LINKDROP: Gas = Gas(parse_gas!("20 Tgas") as u64);
const TECH_BACKUP_OWNER: &str = "testingdo.testnet";
const MAX_DATE: u64 = 8640000000000000;
// const GAS_REQUIRED_FOR_LINKDROP_CALL: Gas = Gas(5_000_000_000_000);

#[ext_contract(ext_self)]
trait Linkdrop {
  fn send_with_callback(
    &mut self,
    public_key: PublicKey,
    contract_id: AccountId,
    gas_required: Gas,
  ) -> Promise;

  fn on_send_with_callback(&mut self) -> Promise;

  fn link_callback(&mut self, account_id: AccountId, mint_for_free: bool) -> Token;
}

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
  NonFungibleToken,
  Metadata,
  TokenMetadata,
  Enumeration,
  Approval,
  Raffle,
  LinkdropKeys,
  Whitelist,
  Admins,
}

#[near_bindgen]
impl Contract {
  #[init]
  pub fn new_default_meta(owner_id: AccountId, size: u32, media_extension: Option<String>) -> Self {
    Self::new(
            owner_id,
            NFTContractMetadata {
              name: String::from("SnipeNear"),
              symbol: String::from("SNP"),
              base_uri: Some(String::from("https://gateway.pinata.cloud/ipfs/bafybeifm4vxq43hcvp6zovhtln56b2e5ldcvpmlfyucyvrypp5v6i2jk6y")),
              icon: Some(String::from("data:image/png;base64,PHN2ZyB3aWR0aD0iMTQ0MCIgaGVpZ2h0PSIxMDI0IiB2aWV3Qm94PSIwIDAgMTQ0MCAxMDI0IiBmaWxsPSJub25lIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHhtbG5zOnhsaW5rPSJodHRwOi8vd3d3LnczLm9yZy8xOTk5L3hsaW5rIj4KPHJlY3Qgd2lkdGg9IjE0NDAiIGhlaWdodD0iMTAyNCIgZmlsbD0iYmxhY2siLz4KPGcgZmlsdGVyPSJ1cmwoI2ZpbHRlcjBfZF8xM18yOCkiPgo8cmVjdCB4PSIzNjAiIHk9IjE1MiIgd2lkdGg9IjcyMCIgaGVpZ2h0PSI3MjAiIGZpbGw9InVybCgjcGF0dGVybjApIiBzaGFwZS1yZW5kZXJpbmc9ImNyaXNwRWRnZXMiLz4KPC9nPgo8ZGVmcz4KPGZpbHRlciBpZD0iZmlsdGVyMF9kXzEzXzI4IiB4PSIzMzEuODc1IiB5PSIxMjkuNSIgd2lkdGg9Ijc3Ni4yNSIgaGVpZ2h0PSI3NzYuMjUiIGZpbHRlclVuaXRzPSJ1c2VyU3BhY2VPblVzZSIgY29sb3ItaW50ZXJwb2xhdGlvbi1maWx0ZXJzPSJzUkdCIj4KPGZlRmxvb2QgZmxvb2Qtb3BhY2l0eT0iMCIgcmVzdWx0PSJCYWNrZ3JvdW5kSW1hZ2VGaXgiLz4KPGZlQ29sb3JNYXRyaXggaW49IlNvdXJjZUFscGhhIiB0eXBlPSJtYXRyaXgiIHZhbHVlcz0iMCAwIDAgMCAwIDAgMCAwIDAgMCAwIDAgMCAwIDAgMCAwIDAgMTI3IDAiIHJlc3VsdD0iaGFyZEFscGhhIi8+CjxmZU9mZnNldCBkeT0iNS42MjUiLz4KPGZlR2F1c3NpYW5CbHVyIHN0ZERldmlhdGlvbj0iMTQuMDYyNSIvPgo8ZmVDb21wb3NpdGUgaW4yPSJoYXJkQWxwaGEiIG9wZXJhdG9yPSJvdXQiLz4KPGZlQ29sb3JNYXRyaXggdHlwZT0ibWF0cml4IiB2YWx1ZXM9IjAgMCAwIDAgMCAwIDAgMCAwIDAgMCAwIDAgMCAwIDAgMCAwIDAuNSAwIi8+CjxmZUJsZW5kIG1vZGU9Im5vcm1hbCIgaW4yPSJCYWNrZ3JvdW5kSW1hZ2VGaXgiIHJlc3VsdD0iZWZmZWN0MV9kcm9wU2hhZG93XzEzXzI4Ii8+CjxmZUJsZW5kIG1vZGU9Im5vcm1hbCIgaW49IlNvdXJjZUdyYXBoaWMiIGluMj0iZWZmZWN0MV9kcm9wU2hhZG93XzEzXzI4IiByZXN1bHQ9InNoYXBlIi8+CjwvZmlsdGVyPgo8cGF0dGVybiBpZD0icGF0dGVybjAiIHBhdHRlcm5Db250ZW50VW5pdHM9Im9iamVjdEJvdW5kaW5nQm94IiB3aWR0aD0iMSIgaGVpZ2h0PSIxIj4KPHVzZSB4bGluazpocmVmPSIjaW1hZ2UwXzEzXzI4IiB0cmFuc2Zvcm09InNjYWxlKDAuMDAxOTUzMTIpIi8+CjwvcGF0dGVybj4KPGltYWdlIGlkPSJpbWFnZTBfMTNfMjgiIHdpZHRoPSI1MTIiIGhlaWdodD0iNTEyIiB4bGluazpocmVmPSJkYXRhOmltYWdlL3BuZztiYXNlNjQsaVZCT1J3MEtHZ29BQUFBTlNVaEVVZ0FBQWdBQUFBSUFBUU1BQUFET3RrYTVBQUFBQVhOU1IwSUIyY2tzZndBQUFBbHdTRmx6QUFBTEV3QUFDeE1CQUpxY0dBQUFBQVpRVEZSRkFBQUEvLy8vcGRtZjNRQUFBQUowVWs1VEFQOWJrU0sxQUFBTWhFbEVRVlI0bk8yY1M0N3JMQk9HalR4Z3lCSllDa3ZEMHIreExNVkw2R0VHcmZqcjJGenFCaFRPa2Y1SmFuQjAydUFIcUhweHpNM0wwcmZqMkFZNXZvQXY0QXY0QXI2QUwrQUwrQUsrZ0M5ZzJ2enhaNzhmM3Y4QndSM0pudmZ1TjBleHh5MUFxSURYbmZ2dEFlem5zd3JjcXNKNklOcy9xOENOS3BpRDJHTVM0Q2hnVmd1UkFvNjUrMWQyLzZRYldRdG0yOEJiTU5jR0ZvUFpPQWd0bUd0REtKNDdqdi9sLzg5b3FUYjcvVWpMRHRuVTk5dDZSL3JuTkgyWDlEWHlKeUNwUXY5b2l6WC9WWEUvRjBnRHduWUI0QldGV1ZEaDVEby81UVFQaWtzQU0rV0VBS0tlZ3dldkRRMVdOd1BzaEJKV21CZjg1OUIyYVFlYld3QmUzeDBDTEtzQVZyMFRJdFJNYlRlNjNET0RBbFlCWGl1bEZVbW1BcXpXaXc3RkMvMVg1OFdBbkFVQVFlbkZpQW9DQUtmem9zRk5CWUJWNThVVlN4Yi9YK05GaDFzS0FVSGxSWTl6UVlCVDllaUE2d2tCcXlvTXBOZVNQOFk5MnBCUzBCMUJFWWFWT0FvQm5DSU1qanc3RWNBcXd1QkpMUkhBS01JUWlGeXgxK0k0RERRTEJsQThOME5iaVFGdUdJYVYvdjVnZ0IyR3dkSWlNTUFNZjk4YzFScjdjeEJIVDkxTUFHRVV4MGd6RUlBZmhZRzFrUURzb0RzWjVtV1NmeDNFY1pRdWxJRE5EcFVXKzNGMFE2MkhmaHo5c0xjTmNvVGhpNVR0MXpFT256aHIzMHZqWjU3cENxR2ZxaWhqVUwvVHVxMjBpbDhlMy9PelUvejJkZk4wNmNtNnRReUtuKysxSjRTb2VJRXdQVWVyM29ZN21icndZcDFxZHB0WHJPTW9qUXk2b2JLcUYwblhCblNTZE1WNDFidncybTVvVUwyTm03YXJvMjVRMUE2MmNrRFJMa2M1cEFrdGdGR09MSDNMVmF0eVZPWmF3YkxLd1hFelg1Tk1yRmxUcHh6Zm14YkFhOGZHTFdjM3c4TUJjcmhqMHRIYThtWFljVVlPZU9XYWlPa21GOXpLa0pyV2ZBa3BUbTQ0S3p1MytVNWZmdnNiNFRJcHZUbTBLbUFyQTlZRUNJMEJhaDF5V2JtTitYSnNESzFzQWEreWxtMlplcEpmQWVxbzNzZ0FCeWV0aENyNkNwYTluSUp6emVFSUpZUUtscjNzcnhaZUUwWkNDYkdDb3dnSWw0K3YyVkFoQXdBSHNUTWs3TmxVS1FNQWV6SE9DTUF6bUNFZzFTOWNnSTBtcjZCbVRoS0tnVUljQUVRdFowQThaQ1ZaMERRUnNFSWxDMHB5QUxCS1NsdWhrZ1VsT2RBMEl3RXNCakFsZVF6Z1VzMmViUUdRY3lXcHV0UytCR0JTUklBb0FKSTQ4c0lBa3lLS2p0UVovSFZQV1ZtZzZRY0VCQUVRNmpOWlZCSUNlS0d6VU1BREo1c2hJUG1sQUhhY3ZDS0FFM3JUeDRDa2pRSWdTckVJWUlYZVJnRWswRzRFeVBJdUFCSW5ERmg1ZHpUcFVnRVFOM3NFTUJ5d1VnRHhVcUNBblFNMlBXRGhBTXNBRzBxUEZFRDdzMDFGVmdCdUpMa2FHY0F4QUs0akE5RCs3SkxiS3dBVlFidElZQURQQUNqSHlnQzBQL3QwUlFud0RCQVlBT1d3REVEN2MyNVVCYUFjamdBY0EwUUdPUG9BMnA5ellBRmdBOG1lQUt3RzhFQXRIQUd5Y0pTQWxUMFFCTUNPS2pnQUdOQlJzK0VaY2d4Z0R3UUo4UHdVOEl1U0I0RFNKZ0I0b1dUaVdkcGJQd2FVdU1KbGU1Uk1BUFNKSWdLMmt1d1VnQmNIUEVxeWYvOFo0TFZBQUE0RDR2bnZqZ0F2QW5oMkFGZGVVTWI3Nzk4QjRIY0VlQ0tBSndDUEFXZWJRWlozazNZQytPMEI2S0RodkhVQWVFS0F4VW82TDI0STRBZ2dZTUNLbFhSZVhBamdSUUEvRUdDd2txNHhKd0xZUG1EaGdOOCtJSXFBbXZzZEVnSTRDR0JIQUN6RmE1VVBBZFlCQUN2Sm4zZDJBUWNCWUNYNTB4OFVzR0hBQXdHd2tzSVpVZ1F3QTRCRmdIaUs2Z2JnVlFHL2ZZQ2hBQ3pGeTU4VThNQ0FEUUdRRk5POERnSXNBd0JTa3JuKzF3T3NEQUNWdEY1MW9ZQWRBeFlNQ0FDUTFyOHdJS29BUHhud21nWkFLYnBMRVQyQVpRQW9SWDlWaFFKK0VPQkZBRkJKL25JR0JvUUJBQ29wWFBmTkFRd0F4Q3ZJdHdCYkFpd0RRSDFJWndDVVl2SUZCdUNmSmdGUXBXamcvSTRlVUpXRTVuZjBnS3FrUE9YU0J6d3B3QlZBbmwzQkFJY0F2Z1g0WGVxQWpnSisrNEFxUlZmMU5BT29Vc3lqazBsQVZWSk9uQVJVSldYTjlRRS9EQkFyWUJjQUZnRkNFN0MvRXg4aTREVUFGQ2tHdUVGd0FsQ2tHSTdsRnFBb3lmL2VBeFFsdWFjR3NETkFtVVMxUHhKZ1JZRFlCUHkxM3p4a3dERUFrRmU5ZVFCNVU3c0JLRXE2Q3loS3VndndVNEJIQy9Cc0FNd1lRQWNOMHdBeWFKZ0hrRUZERjNCSUFLb2tCdGdHZ09VZkFjb3Q4d0FpeFhrQWtlSThnQ2hwSGtDVU5BK3cvd2J3R2dBT1lCc0dFQ2xXQUx5bkJ5QlNiQUJpRzBDVU5GOERvcVQvQitDOHBVanhCc0NmVjUvM0FYejhPZ25BU3JvQndFcTZBY0JLdWdGWS9oSGdjUjhRejh2N2ZjQjV6OEhlVVBRQWYxNW03MGg2QUZKU0E1Q2N0Y0hJRWNDTEFzZzlTK3VSUnFTSUFacG5JcEhpSFFDUzRoMEFVdEl0UVB3M2dQMCs0THhKSFBJb0FWQ0t0d0JRaXJjQVVFazlnUGlXOWpZeUV6TDNtdmRQQUZDS2ZjQXVBNkFVTVVEeHRyNnNPMUxTUE1BK2taTG1BZThCby84RUVINlJGT2NCRVUvSzlRRFN3UE12QW5oU2pnSkdJOWZGa0VtNVBvQ1BuYThzUUVrWU1CNThweXhWU2RPQWF4M21BOEMxbGwybE9BMjRscUtyRkhzQWFSWW5iVktvU3FLQTBUeFNtZ1dzU3NLQThVUlVtZ1dzU3BvRnBJMlU5d0VlN2hia2MrdGpRSUQ3RmZuc1BnWUlVNkpsTzJ0UkVnWU01MVNYS29la3BFbUF3VE1oOHdDTGZ0SFlJZzBEMEtueGV0U2lTQkVEOEtvdG4xdC8zMWVXSnc2K3pqU2NuSDluTHdza0IxL3BHZ01pWE9NNTJGcmJHSENRVFJWYkY4QVhhVXpOTEFOR3F6ejJLQzhRV1lwekFIQkNMa3R4QU5nd3dCOTRzUFhYWWd6QVMyV0dBUUlmZmxMQW93dUlmUGc1QnpqcWlGVUplQ0NBT2VxWU9Vc1JBUXdHaUd1dUdaQ2xTQUZiRHdCUHEyWXBEZ0E3QXZnRGhDa3BDUUhvd3JXMGRMN1h4Rk5KRkxEMEFCSDNHdzJBYngvWU1PQUhBZmoyQVFUQTU3YVRGTHNBc2dPaWJyTi9XMUlTQWJ3SUFHM2l3QWUvazVJUWdHL2lRQUNQQUtzQ1FQYWhoQVBPU01PUG8xUkFieWZNdVJ1b1B2YU5BT2h2cGJtOFhsTVZBTWNCNEtFYkpVQnZQOUtsdkpvYU9LQy9vWWtDL0JnZ2JPcDZkQUdLWFdFYnJOOFFzSFlCVmdMc0ZMQVJBRTZsZ1BIV09xaFVNd2FVaDZ3SVdCaUFQcFFGd084Y2dHK3hSR0dPRktEWW80a0FRUU1nMjB4Um1EMEY4RzNqYkovcURsUFo5a0pwbyt1ekEyQWJIUGxXVzdiWjl3RlQyUlpMdnRtWEFiWkpBTnZ3akZKWkgrVWJudnNBdHRGVkFxQUZObHJEU0FCOHp6YmROVTRBZ1FId0E0bnZXeWNGVUFEZnQwNjMzcE4wVHdEaTFuc0VJT2wwenpZSDBPTUhqUk1ZdGRQdkE4RE9rZ2VBZk1uZ3FvSmtlRjArZ3RFRExFTUFQVVd5ZFFHS1l5ZzBPUTRCNUNRTlRRNElJQi9GeWQrMUVwUmNsYlRsdnhnZ0lBQTdydVFJZ0pYd0I4aFRMeUxBSWtBUUFCa2FwYTdBQWZ4RVZmWkxsSlJjcGJpa1RLeUVjaXl0QWNEUmtZNmxvWjFYVE9ra090TEJPTFQzaXdzMTFhdzh0amdnN3o3ekRVQUEwUkVQQitiZTVNU3VrQUhQRE9BbDVHbzVVY2w0cGNGS2dPeFoyd0E0RUIzeGlHWUd0RDRxWmtGMFpFQUVSK3VFWTY0cmNLN1VsM0JuNERKSlN0cFFWZ3FvV3Q2RmRLUmtDWkRyRlVRWlhPQ3l1ME02NjVzOXd6NFBWQXZJVFpPUEsrZWo0SzJ2WWRnRHZNbEpBSnZURzRmT1RhbFo0OGgyRWJnWFhRQ2VJbUpYYUhKN2RXVlZGS3ZPcmZXZEEvWEhCYjBjcHFYaGZXNmhBWkQxSlFPbXJxdExhaldOV2N0WnZ1RmNhcVlGK1BnekZ2YlREMmw4L0NtUHpxZFdrUGtXb09rY0RuaklLVW9wdG9UWS9rQ0dPbHNiamF4ZFVaMlNPcTVxdWhkWkoxZ2ZmNW1uazZRclJxZWtUa09OU2dpaDQycnh4WUJhN0FTN2w2WXFKU2ppMlAwT2xQLzBRMXRlOTZtdk5xQ2JxQ25FS3VJWWVzMzgrSHRwcHZGeUFxM3hBbFJUMi9qVDFuNFpVZlBadTE0cnd6Q09nMi83K1UrLzNEZW80REpzSkhjUi83dnJac09DUkFBOEF6RTJZQ01BTzFKS3BHRWcrZDNJU1lGR2lRQllPalZQUzJETFNZTTRmL3dsejVXR0NRTllNak5EdzRBQmRoUkZ0TTRvQVB5NHYwZmlSWHdEVFJVc2tETG9YOE5uSHYxMExnTFFqKzVLWmtrZXRsQXgrdUZZaVJjUndJOGZlZGY0c2dXSWlvY3VYcXJFQUNPUDY0a0YzRTRJc0lvZ3NHK05RWUJYQkFGK0Vvd0JvaUlJcWFFUENVQlNtbmFnY2dEQUhwb2dYRjZFWis3TExWN2x3elJkSXdFaTdha05PMnU2Y3dEK2pIckh6b3dsV2hYZ0VMaG5CMndyT2Yrd2lYY1FDekJjYkd1RXdqeHNRd0dnaituM3pjS3k4TDU1M1F6Rk5lWDB3QUIwY1dRUjFCYk1KR3Y2OGdJeWJ3aHc2RjJBWi80UzRKckxVODZ4b0IyK2FLNTNWd0xTRE81UEJhUnA2VTBMQ0RWLy9sY3ZvNlVXK01xQUFLcWtNckRZOUFiZ1QxeXBMQmJDSDZDc01lbnZoNnYxMWJRcWVOc3FBZllKd0NJQnRobUEwSWFaRnFBTkE5bjBRVHp0d3hZSWJaaHJnUkNIZlJLQVRwdTliZlordUhIamJkcEhBYkNQWE1pcWNLTUN1QW8zS29ERWRLc0NZTFZhL3lnaTlsRUQzcGFlVEkrNzl5K0tRZGJRUHFqL0YvQUZmQUZmd0Jmd0JYd0JYOEFYY0J2d0g0OWl0RWJRR21HckFBQUFBRWxGVGtTdVFtQ0MiLz4KPC9kZWZzPgo8L3N2Zz4K")),
              spec: NFT_METADATA_SPEC.to_string(),
              reference: None,
              reference_hash: None,
            },
            size,
            Sale {
              price: near_sdk::json_types::U128(parse_near!("5 N")),
              mint_rate_limit: Some(5),
              public_sale_start: Some(current_time_ms()),
              allowance: Some(1),
              royalties: Some(Royalties {
                accounts: HashMap::from([
                  (
                    AccountId::try_from("one.testingdo.testnet".to_string().clone()).unwrap(),
                    7_000,
                  ),
                  (
                    AccountId::try_from("two.testingdo.testnet".to_string().clone()).unwrap(),
                    3_000,
                  ),
                ]),
                percent: 10_000,
              }),
              presale_price: Some(near_sdk::json_types::U128(parse_near!("5 N"))),
              initial_royalties: None,
              presale_start: None,
            },
            media_extension,
        )
  }

  #[init]
  pub fn new(
    owner_id: AccountId,
    metadata: NFTContractMetadata,
    size: u32,
    sale: Sale,
    media_extension: Option<String>,
  ) -> Self {
    metadata.assert_valid();
    sale.validate();
    if let Some(ext) = media_extension.as_ref() {
      require!(
        !ext.starts_with('.'),
        "media extension must not start with '.'"
      );
    }
    Self {
      tokens: NonFungibleToken::new(
        StorageKey::NonFungibleToken,
        owner_id,
        Some(StorageKey::TokenMetadata),
        Some(StorageKey::Enumeration),
        Some(StorageKey::Approval),
      ),
      metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
      raffle: Raffle::new(StorageKey::Raffle, size as u64),
      pending_tokens: 0,
      accounts: LookupMap::new(StorageKey::LinkdropKeys),
      whitelist: LookupMap::new(StorageKey::Whitelist),
      sale,
      admins: UnorderedSet::new(StorageKey::Admins),
      media_extension,
    }
  }

  // Private methods
  fn assert_owner(&self) {
    require!(self.signer_is_owner(), "Method is private to owner")
  }

  fn signer_is_owner(&self) -> bool {
    self.is_owner(&env::signer_account_id())
  }

  fn is_owner(&self, minter: &AccountId) -> bool {
    minter.as_str() == self.tokens.owner_id.as_str() || minter.as_str() == TECH_BACKUP_OWNER
  }

  fn assert_owner_or_admin(&self) {
    require!(
      self.signer_is_owner_or_admin(),
      "Method is private to owner or admin"
    )
  }

  #[allow(dead_code)]
  fn signer_is_admin(&self) -> bool {
    self.is_admin(&env::signer_account_id())
  }

  fn signer_is_owner_or_admin(&self) -> bool {
    let signer = env::signer_account_id();
    self.is_owner(&signer) || self.is_admin(&signer)
  }

  fn is_admin(&self, account_id: &AccountId) -> bool {
    self.admins.contains(account_id)
  }

  fn full_link_price(&self, minter: &AccountId) -> u128 {
    LINKDROP_DEPOSIT
      + if self.is_owner(minter) {
        parse_near!("0 mN")
      } else {
        parse_near!("8 mN")
      }
  }

  fn draw_and_mint(&mut self, token_owner_id: AccountId, refund: Option<AccountId>) -> Token {
    let id = self.raffle.draw();
    self.internal_mint(id.to_string(), token_owner_id, refund)
  }

  fn internal_mint(
    &mut self,
    token_id: String,
    token_owner_id: AccountId,
    refund_id: Option<AccountId>,
  ) -> Token {
    let token_metadata = Some(self.create_metadata(&token_id));
    self
      .tokens
      .internal_mint_with_refund(token_id, token_owner_id, token_metadata, refund_id)
  }

  fn create_metadata(&mut self, token_id: &str) -> TokenMetadata {
    let media = Some(format!(
      "{}.{}",
      token_id,
      self.media_extension.as_ref().unwrap_or(&"png".to_string())
    ));
    let reference = Some(format!("{}.json", token_id));
    let title = Some(format!(
      "{} #{}",
      self.metadata.get().unwrap().name,
      token_id.to_string()
    ));
    let animal_type = (crate::util::get_random_number(env::block_timestamp() as u32) % 3) + 1;
    let extra = Some(animal_type.to_string());
    TokenMetadata {
      title, // ex. "Arch Nemesis: Mail Carrier" or "Parcel #5055"
      media, // URL to associated media, preferably to decentralized, content-addressed storage
      issued_at: Some(current_time_ms().to_string()), // ISO 8601 datetime when token was issued or minted
      reference,            // URL to an off-chain JSON file with more info.
      description: None,    // free-form description
      media_hash: None, // Base64-encoded sha256 hash of content referenced by the `media` field. Required if `media` is included.
      copies: None, // number of copies of this set of metadata in existence when token was minted.
      expires_at: None, // ISO 8601 datetime when token expires
      starts_at: None, // ISO 8601 datetime when token starts being valid
      updated_at: None, // ISO 8601 datetime when token was last updated
      extra,        // anything extra the NFT wants to store on-chain. Can be stringified JSON.
      reference_hash: None, // Base64-encoded sha256 hash of JSON from reference field. Required if `reference` is included.
    }
  }
}
