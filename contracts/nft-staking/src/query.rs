#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Env, StdResult, Deps, QueryRequest, WasmQuery, StdError, Order};
use cw20::Expiration;
use cw721::{Cw721QueryMsg, AllNftInfoResponse, OwnerOfResponse, NftInfoResponse, Approval};
use cw721_base::Extension;
use crate::handler::{compute_rewards, staker_tokenid_key, query_rewards_token_balance, get_cycle, get_period};
use crate::msg::{QueryMsg, ConfigResponse, StartTimeResponse, TotalRewardsPoolResponse, StakerHistoryResponse, TokenInfosResponse, RewardsScheduleResponse, EstimateRewardsResponse, NextClaimResponse, WithdrawRewardsPoolResponse, DisableResponse, NumberOfStakedNftsResponse, StakedAllNftInfoResponse, MaxComputePeriodResponse, StakedNftsByOwnerResponse, TokenInfoMsg, GetGrantsResponse, UnbondingDurationResponse, GetCurrentCycleAndPeriodResponse};
use crate::state::{CONFIG_STATE, REWARDS_SCHEDULE, START_TIMESTAMP, DISABLE, TOTAL_REWARDS_POOL, STAKER_HISTORIES, TOKEN_INFOS, NEXT_CLAIMS, NUMBER_OF_STAKED_NFTS, MAX_COMPUTE_PERIOD, GRANTS, Grant, UNBONDING_DURATION};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(
    deps: Deps,
    env: Env,
    msg: QueryMsg
) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&get_config(deps)?),
        QueryMsg::GetCurrentCycleAndPeriod {} => to_binary(&get_current_cycle_and_period(deps, env)?),
        QueryMsg::GetAllGrants {} => to_binary(&get_all_grants(deps)?),
        QueryMsg::GetRewardsSchedule {} => to_binary(&get_rewards_schedule(deps)?),
        QueryMsg::GetMaxComputePeriod {} => to_binary(&get_max_compute_period(deps)?),
        QueryMsg::GetUnbondingDuration {} => to_binary(&get_unbonding_duration(deps)?),
        QueryMsg::StartTime {} => to_binary(&start_time(deps, env)?),
        QueryMsg::Disable {} => to_binary(&disable(deps)?),
        QueryMsg::TotalRewardsPool {} => to_binary(&total_rewards_pool(deps)?),
        QueryMsg::WithdrawRewardsPoolAmount {} => to_binary(&withdraw_rewards_pool_amount(deps, env)?),
        QueryMsg::StakerHistory { staker, token_id } => to_binary(&staker_history(deps, staker, token_id)?),
        QueryMsg::TokenInfo { token_id } => to_binary(&token_infos(deps, env, token_id)?),
        QueryMsg::EstimateRewards { periods, staker, token_id } => to_binary(&estimate_rewards(deps, env, periods, token_id, staker)?),
        QueryMsg::NextClaim { staker, token_id } => to_binary(&next_claims(deps, staker, token_id)?),
        QueryMsg::NumberOfStakedNfts {} => to_binary(&number_of_staked_nfts(deps)?),
        QueryMsg::StakedAllNftInfo { token_id } => to_binary(&staked_all_nft_info(deps, token_id)?),
        QueryMsg::StakedNftsByOwner { staker } => to_binary(&staked_nfts_by_owner(deps, staker)?),
    }
}

// query configuration.
fn get_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config_state = CONFIG_STATE.load(deps.storage)?;
    Ok(ConfigResponse { 
        owner: config_state.owner.to_string(), 
        cycle_length_in_seconds: config_state.cycle_length_in_seconds,
        period_length_in_cycles: config_state.period_length_in_cycles,
        white_listed_nft_contract: config_state.white_listed_nft_contract.to_string(),
        rewards_token_contract: config_state.rewards_token_contract.to_string(),
    })
}

// query current cycle and period.
fn get_current_cycle_and_period(
    deps: Deps,
    env: Env,
) -> StdResult<GetCurrentCycleAndPeriodResponse> {
    let current_cycle: u64;
    let current_period: u64;
    let timestamp = env.block.time.seconds();

    let start_timestamp = START_TIMESTAMP.may_load(deps.storage)?;
    if start_timestamp.is_none() {
        return Ok(GetCurrentCycleAndPeriodResponse::not_started())
    }

    let config = CONFIG_STATE.load(deps.storage)?;

    let get_cycle = get_cycle(timestamp, start_timestamp.unwrap(), config.clone());
    match get_cycle {
        Ok(t) => {
            current_cycle = t;
        },
        Err(e) => {
            return Ok(GetCurrentCycleAndPeriodResponse::with_err(e))
        }
    }

    let get_period = get_period(current_cycle, config);
    match get_period {
        Ok(t) => {
            current_period = t;
        },
        Err(e) => {
            return Ok(GetCurrentCycleAndPeriodResponse::with_err(e))
        }
    }

    Ok(GetCurrentCycleAndPeriodResponse::new(current_cycle, current_period))
}

// query granted addresses.
fn get_all_grants(
    deps: Deps,
) -> StdResult<GetGrantsResponse> {
    let grants: StdResult<Vec<_>> = GRANTS.range(deps.storage, None, None, Order::Ascending).collect();
    match grants {
        Ok(t) => {
            let mut grants: Vec<Grant> = vec![];
            for grant in t {
                grants.append(&mut vec![grant.1]);
            }
            Ok(GetGrantsResponse::new(grants))
        },
        Err(e) => {
            Ok(GetGrantsResponse::with_err(e))
        }
    }

}

// get rewards schedule includes rewards per cycle.
fn get_rewards_schedule(
    deps: Deps
) -> StdResult<RewardsScheduleResponse> {
    let rewards_schedule = REWARDS_SCHEDULE.may_load(deps.storage)?;

    if rewards_schedule.is_none() {
        Ok(RewardsScheduleResponse::none_rewards_schedule())
        
    } else {
        Ok(RewardsScheduleResponse::new(
            rewards_schedule.unwrap(), 
        ))
    }
}

// query value of max compute period. 
fn get_max_compute_period(
    deps: Deps,
) -> StdResult<MaxComputePeriodResponse> {
    let max_compute_period = MAX_COMPUTE_PERIOD.load(deps.storage)?;

    let res = MaxComputePeriodResponse {
        max_compute_period,
    };

    Ok(res)
}

// query unbonding duration.
fn get_unbonding_duration(
    deps: Deps,
) -> StdResult<UnbondingDurationResponse> {
    let unbonding_duration = UNBONDING_DURATION.load(deps.storage)?;

    let res = UnbondingDurationResponse {
        unbonding_duration,
    };

    Ok(res)
}

// get start time after nft staking contract runs start func.
fn start_time(
    deps: Deps,
    env: Env,
) -> StdResult<StartTimeResponse> {
    let now_time = env.block.time.seconds();

    let start_timestamp = START_TIMESTAMP.may_load(deps.storage)?;
    if start_timestamp.is_none() {
        Ok(StartTimeResponse::not_started(now_time))

    } else {
        Ok(StartTimeResponse::new(start_timestamp.unwrap(), now_time))
    }
}

// get disable state.
fn disable(
    deps: Deps,
) -> StdResult<DisableResponse> {
    let start_timestamp = START_TIMESTAMP.may_load(deps.storage)?;
    if start_timestamp.is_none() {
        Ok(DisableResponse::not_started())

    } else {
        let disable = DISABLE.load(deps.storage)?;
        Ok(DisableResponse::new(disable))
    }
}

// get total supplied rewards pool.
fn total_rewards_pool (
    deps: Deps,
) -> StdResult<TotalRewardsPoolResponse> {
    let total_rewards_pool = TOTAL_REWARDS_POOL.may_load(deps.storage)?;
    if total_rewards_pool.is_none() {
        Ok(TotalRewardsPoolResponse::empty_rewards_pool())

    } else {
        Ok(TotalRewardsPoolResponse::new(total_rewards_pool.unwrap()))
    }
}

// get current amounts withdrawal rewards pool.
fn withdraw_rewards_pool_amount (
    deps: Deps,
    env: Env,
) -> StdResult<WithdrawRewardsPoolResponse> {
    let address = env.contract.address.to_string();
    let config = get_config(deps)?;

    let balance_response = query_rewards_token_balance(deps, address, config.rewards_token_contract);
    match balance_response {
        Ok(t) => {
            let withdraw_rewards_pool_amount = t.balance.u128();
            Ok(WithdrawRewardsPoolResponse::new(withdraw_rewards_pool_amount))
        },
        Err(e) => {
            Ok(WithdrawRewardsPoolResponse::with_err(e))
        }
    }
}

// get next claims state of staker_tokenid_key.
fn next_claims(
    deps: Deps,
    staker: String,
    token_id: String,
) -> StdResult<NextClaimResponse> {
    let staker_tokenid_key = staker_tokenid_key(staker, token_id);
    let next_claims = NEXT_CLAIMS.may_load(deps.storage, staker_tokenid_key)?;
    if next_claims.is_none() {
        Ok(NextClaimResponse::empty_next_claim())

    } else {
        Ok(NextClaimResponse::new(next_claims.unwrap()))
    }
}

// get staker history.
fn staker_history (
    deps: Deps,
    staker: String,
    token_id: String,
) -> StdResult<StakerHistoryResponse> {

    let staker_tokenid_key = staker_tokenid_key(staker, token_id);
    let staker_history = STAKER_HISTORIES.may_load(deps.storage, staker_tokenid_key.clone())?;

    if staker_history.is_none() {
        Ok(StakerHistoryResponse::have_not_history(staker_tokenid_key))

    } else {
        Ok(StakerHistoryResponse::new(staker_tokenid_key, staker_history.unwrap()))
    }
}

// get token infos retrieved by token ID.
fn token_infos (
    deps: Deps,
    env: Env,
    token_id: String,
) -> StdResult<TokenInfosResponse> {
    let token_infos = TOKEN_INFOS.may_load(deps.storage, token_id.clone())?;

    if token_infos.is_none() {
        Ok(TokenInfosResponse::invalid_token_id(token_id))

    } else {
        if token_infos.clone().unwrap().is_staked {
            Ok(TokenInfosResponse::new(deps, env, token_id, token_infos.unwrap()))

        } else {
            Ok(TokenInfosResponse::unstaked_token_id(token_id, token_infos.unwrap()))
        }
    }
}

// get calculated current rewards of staker_tokenid_key.
pub fn estimate_rewards(
    deps: Deps,
    env: Env,
    periods: u64,
    token_id: String,
    staker: String,
) -> StdResult<EstimateRewardsResponse> {
    let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());
    
    let start_timestamp = START_TIMESTAMP.may_load(deps.storage)?;
    if start_timestamp.is_none() {
        return Ok(EstimateRewardsResponse::not_started(staker_tokenid_key))
    }

    let disable = DISABLE.load(deps.storage)?;
    if disable == true {
        return Ok(EstimateRewardsResponse::disabled(staker_tokenid_key))
    }

    let next_claim = NEXT_CLAIMS.may_load(deps.storage, staker_tokenid_key.clone())?;
    if next_claim.is_none() {
        return Ok(EstimateRewardsResponse::invalid_claim(staker_tokenid_key))
    }

    let config = CONFIG_STATE.load(deps.storage)?;
    let now = env.block.time.seconds();

    let compute_rewards = compute_rewards(deps, staker_tokenid_key.clone(), periods, now, start_timestamp.unwrap(), config.clone(), token_id);
    match compute_rewards {
        Ok(t) => {
            let claim = t.0;
            Ok(EstimateRewardsResponse::new(staker_tokenid_key, claim))
        },
        Err(e) => {
            Ok(EstimateRewardsResponse::with_err(staker_tokenid_key, e))
        }
    }
}

// get the number of staked nfts in the nft staking contract.
fn number_of_staked_nfts(
    deps: Deps,
) -> StdResult<NumberOfStakedNftsResponse> {
    let start_timestamp = START_TIMESTAMP.may_load(deps.storage)?;
    if start_timestamp.is_none() {
        return Ok(NumberOfStakedNftsResponse::not_started())
    }

    let number_of_staked_nfts = NUMBER_OF_STAKED_NFTS.load(deps.storage)?;
    Ok(NumberOfStakedNftsResponse::new(number_of_staked_nfts))
}

// get staked nfts info by querying AllNftInfo of whitelisted nft contract.
fn staked_all_nft_info(
    deps: Deps,
    token_id: String,
) -> StdResult<StakedAllNftInfoResponse<Extension>> {
    let config = get_config(deps)?;
    
    let all_nft_info: Result<AllNftInfoResponse::<Extension>, StdError>  = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart{
        contract_addr: config.white_listed_nft_contract,
        msg: to_binary(&Cw721QueryMsg::AllNftInfo { 
            token_id, 
            include_expired: Some(true),
        })?,
    }));

    match all_nft_info {
        Ok(t) => {
            Ok(StakedAllNftInfoResponse::new(t))
        },
        Err(e) => {
            let empty_approval: Vec<Approval> = vec![Approval{
                spender:"".to_string(), 
                expires: Expiration::default(),
            }];

            let empty_res = AllNftInfoResponse {
                access: OwnerOfResponse {
                    owner: "".to_string(),
                    approvals: empty_approval,
                },

                info: NftInfoResponse {
                    token_uri: Some("".to_string()),
                    extension: Extension::None,
                }
            };
            Ok(StakedAllNftInfoResponse::with_err(empty_res, e))
        }            
    }
}

// the number of nfts which are staked by the staker.
pub fn staked_nfts_by_owner(
    deps: Deps,
    staker: String,
) -> StdResult<StakedNftsByOwnerResponse> {

    let token_infos: StdResult<Vec<_>> = TOKEN_INFOS.range(deps.storage, None, None, Order::Ascending).collect();
    match token_infos {
        Ok(t) => {
            let mut staked_nfts: Vec<TokenInfoMsg> = vec![];
            for token_info in t {
                if token_info.1.owner == staker {
                    let info = TokenInfoMsg {
                        token_id: token_info.0,
                        token_info: token_info.1,
                    };
                    staked_nfts.append(&mut vec![info])
                }
            }
            Ok(StakedNftsByOwnerResponse::new(staked_nfts))
        },
        Err(e) => {
            let empty_response = vec![TokenInfoMsg::default()];

            Ok(StakedNftsByOwnerResponse::with_err(empty_response, e))
        }
    }
}