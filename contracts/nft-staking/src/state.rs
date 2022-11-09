use std::str::FromStr;

use cw20::Expiration;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::{Addr, DepsMut, MessageInfo};
use cw_storage_plus::{Item, Map};

use crate::ContractError;

pub const UNSPECIFIED: &str = "BOND_STATUS_UNSPECIFIED";
pub const UNBONDED: &str = "BOND_STATUS_UNBONDED";
pub const UNBONDING: &str = "BOND_STATUS_UNBONDING";
pub const BONDED: &str = "BOND_STATUS_BONDED";

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

impl Snapshot {
    pub fn default() -> Self {
        Snapshot { is_staked: false, start_cycle: 0 }
    }

    pub fn new(
        is_staked: bool,
        start_cycle: u64,
    ) -> Self {
        Snapshot { is_staked, start_cycle }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenInfo {
    pub owner: String,
    pub is_staked: bool,
    pub deposit_cycle: u64,
    pub withdraw_cycle: u64,
    pub bond_status: String,
    pub req_unbond_time: u64,
}

impl TokenInfo {
    pub fn default() -> Self {
        TokenInfo { 
            owner: String::from_str("").unwrap(), 
            is_staked: false, 
            deposit_cycle: 0, 
            withdraw_cycle: 0,
            bond_status: UNSPECIFIED.to_string(),
            req_unbond_time: 0,
        }
    }

    pub fn stake(
        owner: String,
        is_staked: bool,
        deposit_cycle: u64,
    ) -> Self {
        TokenInfo { 
            owner, 
            is_staked, 
            deposit_cycle, 
            withdraw_cycle: 0,
            bond_status: BONDED.to_string(),
            req_unbond_time: 0,
        }
    }

    pub fn unstake_unbonding(
        owner: String,
        is_staked: bool,
        deposit_cycle: u64,
        withdraw_cycle: u64,
        req_unbond_time: u64,
    ) -> Self {
        TokenInfo { 
            owner, 
            is_staked, 
            deposit_cycle, 
            withdraw_cycle,
            bond_status: UNBONDING.to_string(),
            req_unbond_time,
        }
    }

    pub fn unstake_unbonded(
        owner: String,
        is_staked: bool,
        deposit_cycle: u64,
        withdraw_cycle: u64,
        req_unbond_time: u64,
    ) -> Self {
        TokenInfo { 
            owner, 
            is_staked, 
            deposit_cycle, 
            withdraw_cycle,
            bond_status: UNBONDED.to_string(),
            req_unbond_time,
        }
    }
    pub fn unstake(
        is_staked: bool,
        deposit_cycle: u64,
        withdraw_cycle: u64,
    ) -> Self {
        TokenInfo { 
            owner: String::from_str("").unwrap(), 
            is_staked, 
            deposit_cycle, 
            withdraw_cycle,
            bond_status: UNSPECIFIED.to_string(),
            req_unbond_time: 0,
        }
    }

    // check message sender is nft owner which records in the TOKEN_INFOs state.
    pub fn check_staker(
        deps: DepsMut,
        info: MessageInfo,
        token_id: String,
    ) -> Result<Self, ContractError> {
        let token_info = TOKEN_INFOS.may_load(deps.storage, token_id)?;
        if token_info.is_none() {
            return Err(ContractError::InvalidTokenId {})
        }

        if token_info.clone().unwrap().owner != info.sender.clone() {
            return Err(ContractError::InvalidNftOwner{
                requester: info.sender.to_string(),
                nft_owner: token_info.unwrap().owner,
            })
        }

        Ok(token_info.unwrap())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct NextClaim {
    pub period: u64,
    pub staker_snapshot_index: u64,
}

impl NextClaim {
    pub fn default() -> Self {
        NextClaim { 
            period: 0,
            staker_snapshot_index: 0,
        }
    }

    pub fn new(
        period: u64,
        staker_snapshot_index: u64,
    ) -> Self {
        NextClaim { period, staker_snapshot_index }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Claim {
    pub start_period: u64,
    pub periods: u64,
    pub amount: u128,
}

impl Claim {
    pub fn default() -> Self {
        Claim { start_period: 0, periods: 0, amount: 0 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Grant {
    pub address: String,
    pub expires: Expiration,
}

impl Grant {
    pub fn new(
        address: String,
        expires: Option<Expiration>,
    ) -> Self {
        let expires_data: Expiration;
        if expires.is_none() {
            expires_data = Expiration::default()
        } else {
            expires_data = expires.unwrap()
        }
        Grant { address, expires: expires_data }
    }
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
pub const GRANTS: Map<String, Grant> = Map::new("grant");
pub const UNBONDING_DURATION: Item<u64> = Item::new("unbonding_duration");