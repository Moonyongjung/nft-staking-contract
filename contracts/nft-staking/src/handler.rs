use std::{ops::Add, str::FromStr};

use cosmwasm_std::{DepsMut, Uint128, Addr, CosmosMsg, to_binary, WasmMsg, MessageInfo, QueryRequest, WasmQuery, Deps, Coin, Env};
use cw20::{Cw20ExecuteMsg, Cw20QueryMsg, BalanceResponse, Cw20ReceiveMsg};
use cw721::{Cw721ExecuteMsg};

use crate::{state::{Config, Snapshot, STAKER_HISTORIES, START_TIMESTAMP, DISABLE, NEXT_CLAIMS, Claim, REWARDS_SCHEDULE, NextClaim, NUMBER_OF_STAKED_NFTS, MAX_COMPUTE_PERIOD, GRANTS, TOKEN_INFOS, UNBONDING, TokenInfo, UNBONDING_DURATION, UNBONDED}, ContractError, msg::{UpdateHistoriesMsg}};

pub const CHECK_REWARDS_POOL_AIM_EMPTY: &str = "check_empty_rewards_pool";
pub const CHECK_REWARDS_POOL_AIM_INSUFFICIENT: &str = "check_insufficient_rewards_pool";
pub const CHECK_REWARDS_POOL_AIM_BOTH: &str = "both";
pub const IS_STAKED: bool = true;
const MIN_CYCLE_LENGTH: u64 = 10;
const MIN_PERIOD: u64 = 2;

// get current period.
pub fn get_current_period(
    now: u64,
    start_timestamp: u64,
    config: Config,
) -> Result<u64, ContractError> {
    let cycle = get_cycle(now, start_timestamp, config.clone())?;
    let current_period = get_period(cycle, config)?;

    Ok(current_period)
}

// get period of this cycle.
pub fn get_period(
    cycle: u64,
    config: Config,
) -> Result<u64, ContractError> {
    if cycle == 0 {
        return Err(ContractError::CycleNotZero {})
    }

    Ok((cycle - 1) / config.period_length_in_cycles + 1)
}

// get cycle of this timestamp.
pub fn get_cycle(
    timestamp: u64,
    start_timestamp: u64,
    config: Config,
) -> Result<u64, ContractError> {
    if timestamp < start_timestamp {
        return Err(ContractError::TimestampPreceesContractStart {})
    }
    
    Ok((timestamp - start_timestamp) / config.cycle_length_in_seconds + 1)
}

// validate of cycle length.
pub fn is_valid_cycle_length(
    cycle_length_in_seconds: u64,
) -> Result<bool, ContractError> {
    // cycle length must be longer than MIN_CYCLE_LENGTH.  
    if cycle_length_in_seconds < MIN_CYCLE_LENGTH {
        return Err(ContractError::CycleLengthInvalid { 
            min_cycle_length: MIN_CYCLE_LENGTH,
            cycle_length_in_seconds 
        })
    } else {
        let res = true;
        Ok(res)
    }    
}

// validate of period length.
pub fn is_valid_period_length(
    period_length_in_cycles: u64,
) -> Result<bool, ContractError> {
    // period length must be longer than MIN_PERIOD.
    if period_length_in_cycles < MIN_PERIOD {
        return Err(ContractError::PeriodLengthInvalid { 
            min_period: MIN_PERIOD,
            period_length_in_cycles 
        })
    } else {
        let res = true;
        Ok(res)
    }
}

// make contract message info.
pub fn contract_info(
    msg: Cw20ReceiveMsg,
) -> Result<MessageInfo, ()>{
    let contract_owner_info = MessageInfo {
        sender: Addr::unchecked(msg.sender.clone()),
        funds: [Coin::default()].to_vec(),
    }; 

    Ok(contract_owner_info)
}

// mapping staker and nft token id in order to use state key.
pub fn staker_tokenid_key(
    staker: String,
    token_id: String,
) -> String {
    let staker_tokenid_key = staker.add("@").add(&token_id.clone());
    return staker_tokenid_key
}

// check message sender is contract owner.
pub fn check_contract_owner_only (
    info: MessageInfo, 
    config: Config,
) -> Result<bool, ContractError> {
    if config.owner != info.sender.to_string() {
        return Err(ContractError::Unauthorized {})
    }

    Ok(true)
}

// check message sender is contract owner or granted address.
pub fn check_contract_owner(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
) -> Result<bool, ContractError> {
    // contract owner.
    if config.owner == info.sender.to_string() {
        return Ok(true)    
    }

    // granted address by adding contract owner.
    let grants = GRANTS.may_load(deps.storage, info.sender.to_string())?;
    if !grants.is_none() && !grants.unwrap().expires.is_expired(&env.block) {
        return Ok(true)
    }

    Err(ContractError::Unauthorized {})
}

// check the contract is started and return start timestamp.
pub fn check_start_timestamp(
    deps: DepsMut,
) -> Result<u64, ContractError> {
    let start_timestamp = START_TIMESTAMP.may_load(deps.storage)?;
    if start_timestamp.is_none() {
        return Err(ContractError::NotStarted {})
    }

    Ok(start_timestamp.unwrap())
}

// check the contract is disabled.
pub fn check_disable(
    deps: DepsMut,
) -> Result<bool, ContractError> {
    let disable = DISABLE.load(deps.storage)?;
    if disable == true {
        return Err(ContractError::Disabled {})
    }

    Ok(disable)
}

// check unbonding status.
pub fn check_unbonding_end(
    deps: Deps,   
    token_info: TokenInfo,
    timestamp: u64,
) -> Result<bool, ContractError> {
    let unbonding_duration = UNBONDING_DURATION.load(deps.storage)?;
    if !(token_info.bond_status == UNBONDING && timestamp > token_info.req_unbond_time + unbonding_duration) {
        return Err(ContractError::NotReachUnbondingTime {})
    }

    Ok(true)
}

// check empty rewards pool of nft staking contract.
pub fn check_rewards_pool_balance(
    deps: DepsMut,
    env: Env,
    config: Config,
    aim: &str,
    amount: Option<u128>
) -> Result<(), ContractError> {
    let address = env.contract.address.to_string();
    let rewards_token_contract = config.clone().rewards_token_contract;
    let balance_response = query_rewards_token_balance(deps.as_ref(), address.clone(), rewards_token_contract.clone())?;

    if aim == CHECK_REWARDS_POOL_AIM_EMPTY || aim == CHECK_REWARDS_POOL_AIM_BOTH {
        if balance_response.balance == Uint128::from_str("0").unwrap() {
            return Err(ContractError::EmptyRewardsPool {})
        } 
    } 
    
    if aim == CHECK_REWARDS_POOL_AIM_INSUFFICIENT || aim == CHECK_REWARDS_POOL_AIM_BOTH {
        let amount = amount.unwrap();
        if balance_response.balance.u128() < amount {
            return Err(ContractError::InsufficientRewardsPool { 
                rewards_pool_balance: balance_response.balance.u128(), 
                claim_amount: amount, 
            })
        }
    }

    Ok(())
}

// execute token transfer.
pub fn execute_token_contract_transfer(
    rewards_token_contract: String,
    recipient: String,
    amount: u128,
) -> Result<Vec<CosmosMsg>, ContractError> {
    let mut messages: Vec<CosmosMsg> = vec![];
    let u128_amount = Uint128::from(amount);

    let transfer_from: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: rewards_token_contract.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer { 
            recipient, 
            amount: u128_amount,
        })?,
        funds: vec![]
    });
    messages.push(transfer_from);

    Ok(messages)
}

// execute transfer nft for replacing owner when unstake.
pub fn execute_transfer_nft_unstake(
    token_id: String,
    staker: String,
    nft_contract: String,
) -> Result<CosmosMsg, ContractError> {
    let transfer_from: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: nft_contract,
        msg: to_binary(&Cw721ExecuteMsg::TransferNft { 
            recipient: staker, 
            token_id, 
        })?,
        funds: vec![]
    });


    Ok(transfer_from)
}

// query rewards token balance.
pub fn query_rewards_token_balance(
    deps: Deps,
    address: String,
    rewards_token_contract: String,
) -> Result<BalanceResponse, ContractError>{

    let balance_response: BalanceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart{
        contract_addr: rewards_token_contract,
        msg: to_binary(&Cw20QueryMsg::Balance { 
            address 
        })?,
    }))?;

    Ok(balance_response)
}

// update history of staker at the current cycle with a new difference in stake.
pub fn update_histories(
    mut deps: DepsMut,
    staker_tokenid_key: String,
    is_staked: bool,
    current_cycle: u64,
) -> Result<UpdateHistoriesMsg, ContractError> {
    let staker_snapshot_index = update_staker_history(deps.branch(), is_staked, current_cycle, staker_tokenid_key.clone())?;
    let staker_history = STAKER_HISTORIES.may_load(deps.branch().storage, staker_tokenid_key.clone()).unwrap().unwrap();
    let staker_snapshot = &staker_history[staker_snapshot_index as usize];

    let update_histories_res = UpdateHistoriesMsg {
        staker: staker_tokenid_key,
        current_cycle,
        staker_histories_stake: staker_snapshot.is_staked,
    };

    Ok(update_histories_res)
}

// update history
pub fn update_staker_history(
    deps: DepsMut,
    is_staked: bool,
    current_cycle: u64,
    staker_tokenid_key: String,
) -> Result<u64, ContractError> {
    let staker_history_state = STAKER_HISTORIES.may_load(deps.storage, staker_tokenid_key.clone()).unwrap();

    if !staker_history_state.is_none() {
        let staker_history = staker_history_state.clone().unwrap();
        let history_length = staker_history.len();

        // there is an existing snapshot.
        let snapshot_index = (history_length as u64) - 1;
        let snapshot = &staker_history[snapshot_index as usize];

        if snapshot.start_cycle == current_cycle {
            // update the snapshot if it starts on the current cycle.
            let new_snapshot = Snapshot {
                is_staked,
                start_cycle: current_cycle,
            };

            staker_history.clone().append(&mut vec![new_snapshot]);
            STAKER_HISTORIES.save(deps.storage, staker_tokenid_key, &staker_history)?;

            return Ok(snapshot_index)
        } 
    }

    let new_snapshot = Snapshot {
        is_staked,
        start_cycle: current_cycle,
    };

    let staker_history = vec![new_snapshot];

    // add a new snapshot in the history.
    STAKER_HISTORIES.save(deps.storage, staker_tokenid_key, &staker_history)?;

    Ok(0)
    
}

// calculate the amount of rewards for a staker over a capped number of periods.
pub fn compute_rewards(
    deps: Deps,
    staker_tokenid_key: String,
    periods: u64,
    now: u64,
    start_timestamp: u64,
    config: Config,
    token_id: String,
) -> Result<(Claim, NextClaim), ContractError> {
    let max_compute_period = MAX_COMPUTE_PERIOD.load(deps.storage)?;
    if periods > max_compute_period {
        return Err(ContractError::InvalidMaxPeriod { 
            periods: periods, 
            max_compute_period, 
        })
    }
    let mut claim = Claim::default();
    let mut next_claim = NextClaim::default();

    // computing 0 periods.
    if periods == 0 {
        return Ok((claim, next_claim))
    }

    next_claim = NEXT_CLAIMS.may_load(deps.storage, staker_tokenid_key.clone()).unwrap().unwrap();
    claim.start_period = next_claim.period;

    // nothing has been staked yet.
    if claim.start_period == 0 {
        return Ok((claim, next_claim))
    }

    let mut end_claim_period = get_current_period(now, start_timestamp, config.clone())?;

    let token_info = TOKEN_INFOS.load(deps.storage, token_id)?;
    
    // resitrict constantly supplied rewards after the staker requests unbond.
    // the current period to compute rewards is replaced to requested unbond time.
    if token_info.bond_status == UNBONDING || token_info.bond_status == UNBONDED {
        end_claim_period = get_current_period(token_info.req_unbond_time, start_timestamp, config.clone())?;
    }

    // current period is not claimable.
    if next_claim.period == end_claim_period {
        return Ok((claim, next_claim))
    }

    // retrieve the next snapshots if they exist.
    let staker_history = STAKER_HISTORIES.may_load(deps.storage, staker_tokenid_key.clone()).unwrap().unwrap();

    let s_state_data = staker_history[next_claim.clone().staker_snapshot_index as usize].clone();
    let mut staker_snapshot = Snapshot::new(s_state_data.is_staked, s_state_data.start_cycle);

    let mut next_staker_snapshot = Snapshot::default();
    if next_claim.staker_snapshot_index != staker_history.clone().len() as u64 - 1 {
        let s_data = &staker_history.clone()[(next_claim.staker_snapshot_index + 1) as usize];
        next_staker_snapshot = Snapshot::new(s_data.is_staked, s_data.start_cycle);
    }

    // exclues the current period.
    claim.periods = end_claim_period - next_claim.period;
    if periods < claim.periods {
        claim.periods = periods;
    }

    // re-calibrate the end claim period based on the actual number of periods to claim.
    // next_claim.period will be updated to this value after exiting the loop.
    let end_claim_period = next_claim.period + claim.periods;

    // iterate over periods.
    while next_claim.period != end_claim_period {
        let next_period_start_cycle = next_claim.period * config.clone().period_length_in_cycles + 1;
        let reward_per_cycle = REWARDS_SCHEDULE.may_load(deps.storage).unwrap();
        if reward_per_cycle.is_none() {
            return Err(ContractError::InvalidRewardsSchedule {})
        }
        let reward_per_cycle = reward_per_cycle.unwrap();

        let mut start_cycle = next_period_start_cycle - config.clone().period_length_in_cycles;
        let mut end_cycle = 0;

        // iterate over snapshot.
        while end_cycle != next_period_start_cycle {
            
            // find the range-to-claim start cycle, where the current staker snapshot and the current period overlap.
            if staker_snapshot.start_cycle > start_cycle {
                start_cycle = staker_snapshot.start_cycle;
            }

            // find the range-to-claim ending cycle, where the current staker snapshot and the current period no longer overlap.
            // the end cycle is exclusive of the range-to-claim and represents the beginning cycle of the next range-to-claim.
            end_cycle = next_period_start_cycle;
            if staker_snapshot.is_staked && reward_per_cycle != 0 {
                let snapshot_reward = (end_cycle - start_cycle) as u128 * reward_per_cycle;
                claim.amount = claim.amount.add(snapshot_reward)
            }

            // advance the current staker snapshot to the next (if any) 
            // if its cycle range has been fully processed and if the next snapshot starts at most on next period first cycle.
            if next_staker_snapshot.start_cycle == end_cycle {
                staker_snapshot = next_staker_snapshot;
                next_claim.staker_snapshot_index = next_claim.staker_snapshot_index + 1;

                if next_claim.staker_snapshot_index != (staker_history.len() - 1) as u64 {
                    next_staker_snapshot = staker_history[(next_claim.staker_snapshot_index + 1) as usize];
                } else {
                    next_staker_snapshot = Snapshot::default();
                }
            } 
        }
        next_claim.period = next_claim.period + 1;   
    }

    Ok((claim, next_claim))

}

// manage the number of staked nfts which nft staking contract owns.
pub fn manage_number_nfts(
    deps: DepsMut,
    is_increase: bool,
) {
    let number_of_staked_nfts = NUMBER_OF_STAKED_NFTS.load(deps.storage).unwrap();
    if is_increase {
        NUMBER_OF_STAKED_NFTS.save(deps.storage, &(number_of_staked_nfts + 1)).unwrap();
    } else {
        NUMBER_OF_STAKED_NFTS.save(deps.storage, &(number_of_staked_nfts - 1)).unwrap();
    }
}