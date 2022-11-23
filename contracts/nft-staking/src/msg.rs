use std::str::FromStr;

use cosmwasm_std::{StdError, Env, Deps};
use cw20::{Cw20ReceiveMsg, Expiration};
use cw721::{Cw721ReceiveMsg, AllNftInfoResponse};
use cw721_base::Extension;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{state::{Snapshot, TokenInfo, Claim, NextClaim, Grant, UNBONDING_DURATION, BONDED, UNBONDING, UNBONDED}, ContractError};

pub const SUCCESS: &str = "success";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub cycle_length_in_seconds: u64,
    pub period_length_in_cycles: u64,
    pub white_listed_nft_contract: String,
    pub rewards_token_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    SetConfig(SetConfigMsg),
    Grant {
        address: String,
        expires: Option<Expiration>
    },
    Revoke {
        address: String,
    },
    AddRewardsForPeriods {
        rewards_per_cycle: u128,
    },
    Receive(Cw20ReceiveMsg),
    SetMaxComputePeriod {
        new_max_compute_period: u64,
    },
    SetUnbondingDuration {
        new_unbonding_duration: u64,
    },
    Start {},
    Disable {},
    Enable {},
    WithdrawRewardsPool {
        amount: u128,
    },
    WithdrawAllRewardsPool {},
    ReceiveNft(Cw721ReceiveMsg),
    UnstakeNft {
        token_id: String,
        claim_recipient_address: Option<String>,
    },
    ClaimRewards {
        periods: u64,
        token_id: String,
        claim_recipient_address: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetConfig {},
    GetCurrentCycleAndPeriod {},
    GetAllGrants {},
    GetRewardsSchedule {},
    GetMaxComputePeriod {},
    GetUnbondingDuration {},
    StartTime {},
    Disable {},
    TotalRewardsPool {},
    WithdrawRewardsPoolAmount {},
    StakerHistory {
        staker: String,
        token_id: String,
    },
    TokenInfo {
        token_id: String,
    },
    EstimateRewards {
        periods: u64,
        staker: String,
        token_id: String,
    },
    NextClaim {
        staker: String,
        token_id: String,
    },
    NumberOfStakedNfts {},
    StakedAllNftInfo {
        token_id: String,
    },
    StakedNftsByOwner {
        staker: String,
    }
}

// msgs
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct SetConfigMsg {
    pub cycle_length_in_seconds: Option<u64>,
    pub period_length_in_cycles: Option<u64>,
    pub white_listed_nft_contract: Option<String>,
    pub rewards_token_contract: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TokenInfoMsg {
    pub token_id: String,
    pub token_info: TokenInfo,
}

impl Default for TokenInfoMsg {
    fn default() -> Self {
        TokenInfoMsg { 
            token_id: String::from_str("").unwrap(), 
            token_info: TokenInfo::default() 
        }
    }
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdateHistoriesMsg {
    pub staker: String,
    pub current_cycle: u64,
    pub staker_histories_stake: bool,
}

// responses
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigResponse {
    pub owner: String,
    pub cycle_length_in_seconds: u64,
    pub period_length_in_cycles: u64,
    pub white_listed_nft_contract: String,
    pub rewards_token_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetCurrentCycleAndPeriodResponse {
    pub current_cycle: u64,
    pub current_period: u64,
    pub res_msg: String,
}

impl GetCurrentCycleAndPeriodResponse {
    pub fn new(
        current_cycle: u64,
        current_period: u64,
    ) -> Self {
        GetCurrentCycleAndPeriodResponse { 
            current_cycle, 
            current_period, 
            res_msg: SUCCESS.to_string() 
        }
    }

    pub fn not_started() -> Self {
        GetCurrentCycleAndPeriodResponse { 
            current_cycle: 0,
            current_period: 0, 
            res_msg: ContractError::NotStarted {}.to_string(),
        }
    }

    pub fn with_err(e: ContractError) -> Self {
        GetCurrentCycleAndPeriodResponse { 
            current_cycle: 0,
            current_period: 0,  
            res_msg: e.to_string()  
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetGrantsResponse {
    pub grants: Vec<Grant>,
    pub res_msg: String,
}

impl GetGrantsResponse {
    pub fn new(
        grants: Vec<Grant>
    ) -> Self {
        GetGrantsResponse { 
            grants, 
            res_msg: SUCCESS.to_string()
        }
    }

    pub fn with_err(e: StdError) -> Self {
        GetGrantsResponse { 
            grants: vec![], 
            res_msg: e.to_string()  
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardsScheduleResponse {
    pub rewards_per_cycle: u128,
    pub res_msg: String,
}

impl RewardsScheduleResponse {
    pub fn new(
        rewards_per_cycle: u128,
    ) -> Self {
        RewardsScheduleResponse {
            rewards_per_cycle,
            res_msg: SUCCESS.to_string(),
        }
    }

    pub fn none_rewards_schedule() -> Self {
        RewardsScheduleResponse { 
            rewards_per_cycle: 0, 
            res_msg: ContractError::NoneRewardsSchedule {}.to_string()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MaxComputePeriodResponse {
    pub max_compute_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnbondingDurationResponse {
    pub unbonding_duration: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StartTimeResponse {
    pub start: bool,
    pub start_time : u64,
    pub now_time: u64,
    pub res_msg: String,
}

impl StartTimeResponse {
    pub fn new(
        start_timestamp: u64,
        now_time: u64,
    ) -> Self {
        StartTimeResponse { 
            start: true, 
            start_time: start_timestamp, 
            now_time, 
            res_msg: SUCCESS.to_string(),
        }
    }

    pub fn not_started(now_time: u64) -> Self {
        StartTimeResponse { 
            start: false, 
            start_time: 0, 
            now_time, 
            res_msg: ContractError::NotStarted {}.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct DisableResponse {
    pub disable: bool,
    pub res_msg: String,
}

impl DisableResponse {
    pub fn new(
        disable: bool
    ) -> Self {
        DisableResponse { disable, res_msg: SUCCESS.to_string() }
    }

    pub fn not_started() -> Self {
        DisableResponse { disable: true, res_msg: ContractError::NotStarted {}.to_string() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TotalRewardsPoolResponse {
    pub total_rewards_pool: u128,
    pub res_msg: String,
}

impl TotalRewardsPoolResponse {
    pub fn new(
        total_rewards_pool: u128,
    ) -> Self {
        TotalRewardsPoolResponse { total_rewards_pool, res_msg: SUCCESS.to_string() }
    }

    pub fn empty_rewards_pool() -> Self {
        TotalRewardsPoolResponse { 
            total_rewards_pool: 0, 
            res_msg: ContractError::EmptyRewardsPool {}.to_string() 
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct WithdrawRewardsPoolResponse {
    pub withdraw_rewards_pool_amount: u128,
    pub res_msg: String,
}

impl WithdrawRewardsPoolResponse {
    pub fn new(
        withdraw_rewards_pool_amount: u128,
    ) -> Self {
        WithdrawRewardsPoolResponse {
            withdraw_rewards_pool_amount,
            res_msg: SUCCESS.to_string(),
        }
    }

    pub fn with_err(e: ContractError) -> Self {
        WithdrawRewardsPoolResponse { 
            withdraw_rewards_pool_amount: 0, 
            res_msg: e.to_string() 
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct NextClaimResponse {
    pub next_claim: NextClaim,
    pub res_msg: String,
}

impl NextClaimResponse {
    pub fn new(
        next_claim: NextClaim
    ) -> Self {
        NextClaimResponse { 
            next_claim: next_claim, 
            res_msg: SUCCESS.to_string() 
        }
    }

    pub fn empty_next_claim() -> Self {
        NextClaimResponse { 
            next_claim: NextClaim::default(), 
            res_msg: ContractError::EmptyNextClaim {}.to_string() 
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StakerHistoryResponse {
    pub staker_tokenid_key: String,
    pub staker_history: Vec<Snapshot>,
    pub res_msg: String,
}

impl StakerHistoryResponse {
    pub fn new(
        staker_tokenid_key: String,
        staker_history: Vec<Snapshot>,
    ) -> Self {
        StakerHistoryResponse { 
            staker_tokenid_key, 
            staker_history, 
            res_msg: SUCCESS.to_string() 
        }
    }

    pub fn have_not_history(staker_tokenid_key: String) -> Self {
        StakerHistoryResponse { 
            staker_tokenid_key, 
            staker_history: vec![], 
            res_msg: ContractError::HaveNotHistory {}.to_string() 
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TokenInfosResponse {
    pub token_id: String,
    pub token_info: TokenInfo,
    pub is_reached_status_unbonded: Option<bool>,
    pub res_msg: String,
}

impl TokenInfosResponse {
    pub fn new(
        deps: Deps,
        env: Env,
        token_id: String,
        token_info: TokenInfo,
    ) -> Self {
        let now = env.block.time.seconds();
        let unbonding_duration = UNBONDING_DURATION.load(deps.storage).unwrap();

        let mut status_unbonded: Option<bool> = Some(false);
        let mut token_info_res = token_info.clone();

        if token_info.clone().bond_status == UNBONDING &&
            now > token_info.clone().req_unbond_time + unbonding_duration {

            token_info_res = TokenInfo::unstake_unbonded(
                token_info.clone().owner, 
                token_info.clone().is_staked, 
                token_info.clone().deposit_cycle, 
                token_info.clone().withdraw_cycle, 
                token_info.clone().req_unbond_time
            );
            status_unbonded = Some(true);
        }

        if token_info.clone().bond_status == UNBONDED {
            status_unbonded = Some(true);
        }

        if token_info.clone().bond_status == BONDED {
            status_unbonded = None
        }

        TokenInfosResponse { 
            token_id, 
            token_info: token_info_res,
            is_reached_status_unbonded: status_unbonded,
            res_msg: SUCCESS.to_string() 
        }
    }

    pub fn unstaked_token_id(
        token_id: String,
        token_info: TokenInfo
    ) -> Self {
        TokenInfosResponse { 
            token_id, 
            token_info,
            is_reached_status_unbonded: None, 
            res_msg: ContractError::UnstakedTokenId {}.to_string() 
        } 
    }

    pub fn invalid_token_id(
        token_id: String
    ) -> Self {
        TokenInfosResponse { 
            token_id, 
            token_info: TokenInfo::default(), 
            is_reached_status_unbonded: None,
            res_msg: ContractError::InvalidTokenId {}.to_string() 
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct EstimateRewardsResponse {
    pub req_staker_tokenid_key: String,
    pub claim: Claim,
    pub res_msg: String,
}

impl EstimateRewardsResponse {
    pub fn new(
        req_staker_tokenid_key: String,
        claim: Claim,
    ) -> Self {
        EstimateRewardsResponse { 
            req_staker_tokenid_key, 
            claim, 
            res_msg: SUCCESS.to_string()
        }
    }

    pub fn invalid_claim(
        req_staker_tokenid_key: String
    ) -> Self {
        EstimateRewardsResponse { 
            req_staker_tokenid_key, 
            claim: Claim::default(), 
            res_msg: ContractError::InvalidClaim {}.to_string() 
        }
    }

    pub fn not_started(
        req_staker_tokenid_key: String
    ) -> Self {
        EstimateRewardsResponse { 
            req_staker_tokenid_key, 
            claim: Claim::default(), 
            res_msg: ContractError::NotStarted {}.to_string()
        }
    }

    pub fn disabled(
        req_staker_tokenid_key: String
    ) -> Self {
        EstimateRewardsResponse { 
            req_staker_tokenid_key, 
            claim: Claim::default(), 
            res_msg: ContractError::Disabled {}.to_string() 
        }
    }

    pub fn with_err(
        req_staker_tokenid_key: String,
        e: ContractError,
    ) -> Self {
        EstimateRewardsResponse { 
            req_staker_tokenid_key, 
            claim: Claim::default(), 
            res_msg: e.to_string() 
        }
    }


}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct NumberOfStakedNftsResponse {
    pub number_of_staked_nfts: u128,
    pub res_msg: String,
}

impl NumberOfStakedNftsResponse {
    pub fn new(
        number_of_staked_nfts: u128
    ) -> Self {
        NumberOfStakedNftsResponse { 
            number_of_staked_nfts, 
            res_msg: SUCCESS.to_string()
        }
    }

    pub fn not_started() -> Self {
        NumberOfStakedNftsResponse { number_of_staked_nfts: 0, res_msg: ContractError::NotStarted {}.to_string() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StakedAllNftInfoResponse<T> {
    pub all_nft_info: AllNftInfoResponse<T>,
    pub res_msg: String,
}

impl StakedAllNftInfoResponse<Extension> {
    pub fn new(
        all_nft_info: AllNftInfoResponse<Extension>
    ) -> Self {
        StakedAllNftInfoResponse { 
            all_nft_info, 
            res_msg: SUCCESS.to_string()
        }
    }

    pub fn with_err(
        all_nft_info: AllNftInfoResponse<Extension>,
        e: StdError
    ) -> Self {
        StakedAllNftInfoResponse { 
            all_nft_info, 
            res_msg: e.to_string() 
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct StakedNftsByOwnerResponse {
    pub staked_nfts: Vec<TokenInfoMsg>,
    pub res_msg: String,
}

impl StakedNftsByOwnerResponse {
    pub fn new(
        staked_nfts: Vec<TokenInfoMsg>,
    ) -> Self {
        StakedNftsByOwnerResponse { 
            staked_nfts, 
            res_msg: SUCCESS.to_string() 
        }
    }

    pub fn with_err(
        staked_nfts: Vec<TokenInfoMsg>,
        e: StdError
    ) -> Self {
        StakedNftsByOwnerResponse { 
            staked_nfts, 
            res_msg: e.to_string() 
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

