use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub cycle_length_in_seconds: u64,
    pub period_length_in_cycles: u64,
    pub white_listed_nft_contract: String,
    pub rewards_token_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Copy)]
pub struct Snapshot {
    pub is_staked: bool,
    pub start_cycle: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenInfo {
    pub owner: String,
    pub is_staked: bool,
    pub deposit_cycle: u64,
    pub withdraw_cycle: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NextClaim {
    pub period: u64,
    pub staker_snapshot_index: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Claim {
    pub start_period: u64,
    pub periods: u64,
    pub amount: u128,
}

pub const CONFIG_STATE: Item<Config> = Item::new("config");
pub const START_TIMESTAMP: Item<u64> = Item::new("start_timestamp");
pub const REWARDS_SCHEDULE: Item<u128> = Item::new("rewards_schedule");
pub const TOTAL_REWARDS_POOL: Item<u128> = Item::new("total_rewards_pool");
pub const DISABLE: Item<bool> = Item::new("disable");
pub const STAKER_HISTORIES: Map<String, Vec<Snapshot>> = Map::new("staker_histories");
pub const NEXT_CLAIMS: Map<String, NextClaim> = Map::new("next_claims");
pub const TOKEN_INFOS: Map<String, TokenInfo> = Map::new("token_infos");
pub const NUMBER_OF_STAKED_NFTS: Item<u128> = Item::new("number_of_staked_nfts");
pub const MAX_COMPUTE_PERIOD: Item<u64> = Item::new("max_compute_period");