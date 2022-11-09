use cosmwasm_std::{StdError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized address")]
    Unauthorized {},

    #[error("cycle length is invalid, at least {min_cycle_length} seconds > request {cycle_length_in_seconds} seconds")]
    CycleLengthInvalid {
        min_cycle_length: u64,
        cycle_length_in_seconds: u64,
    },

    #[error("period length is invalid, at least {min_period} cycles > request {period_length_in_cycles} cycles")]
    PeriodLengthInvalid {
        min_period: u64,
        period_length_in_cycles: u64,
    },

    #[error("cycle cannot be zero")]
    CycleNotZero {},

    #[error("timestamp preceeds contract start")]
    TimestampPreceesContractStart {},

    #[error("rewards schedule is null")]
    NoneRewardsSchedule {},

    #[error("already started")]
    AlreadyStarted {},

    #[error("not started, run start()")]
    NotStarted {},

    #[error("disabled")]
    Disabled {},

    #[error("cannot enable, disable state is {disable}")]
    CannotEnable {
        disable: bool,
    },

    #[error("invalid cw20 contract, rewards token contract is {rewards_token_contract}, but request is {requester}")]
    InvalidRewardsTokenContract {
        rewards_token_contract: String,
        requester: String,
    },

    #[error("invalid cw721 contract, whitelisted contract is {white_listed_contract}, but request is {requester}")]
    InvalidWhitelistedContract {
        white_listed_contract: String,
        requester: String,
    },

    #[error("token id is already staked")]
    AlreadyStaked {},

    #[error("unstaked token cooldown")]
    UnstakedTokenCooldown {},

    #[error("invalid token id")]
    InvalidTokenId {},

    #[error("unstaked token id")]
    UnstakedTokenId {},

    #[error("token steel frozen")]
    TokenSteelFrozen {},

    #[error("invalid nft owner, requester is {requester}, but nft owner is {nft_owner}")]
    InvalidNftOwner {
        requester: String,
        nft_owner: String,
    },

    #[error("invalid claim of requester")]
    InvalidClaim {},

    #[error("next claim is empty")]
    EmptyNextClaim {},

    #[error("have no amout for claim")]
    NoAmountClaim {},

    #[error("rewards pool is insufficient to claim, rewards pool balance is {rewards_pool_balance} and claim amount is {claim_amount}")]
    InsufficientRewardsPool {
        rewards_pool_balance: u128,
        claim_amount: u128,
    },

    #[error("have not history")]
    HaveNotHistory {},

    #[error("invalid rewards schedule")]
    InvalidRewardsSchedule {},

    #[error("rewards pool is empty")]
    EmptyRewardsPool {},

    #[error("request claimable periods value for rewards is invalid, request periods: {periods} is bigger than max period: {max_compute_period}")]
    InvalidMaxPeriod {
        periods: u64,
        max_compute_period: u64,
    },

    #[error("invalid set max_compute_period, need bigger than zero")]
    InvalidSetMaxPeriod {},

    #[error("already granted address {address}")]
    AlreadyGranted {
        address: String,
    },

    #[error("invalid granted address {address}")]
    InvalidGrantedAddress {
        address: String,
    },

    #[error("not reach unbonding time")]
    NotReachUnbondingTime {},

    #[error("request token id is under unbonding, or unbonded token id should execute unstake not claim")]
    TokenIdIsUnbonding {},
}