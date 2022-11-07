use std::{ops::Add, str::FromStr};

use cosmwasm_std::{StdResult, DepsMut, Uint128, Addr, CosmosMsg, to_binary, WasmMsg, MessageInfo, QueryRequest, WasmQuery, Deps, Coin};
use cw20::{Cw20ExecuteMsg, Cw20QueryMsg, BalanceResponse, Cw20ReceiveMsg};
use cw721::{Cw721ExecuteMsg};

use crate::{state::{Config, Snapshot, STAKER_HISTORIES, START_TIMESTAMP, DISABLE, TOKEN_INFOS, TokenInfo, NEXT_CLAIMS, Claim, REWARDS_SCHEDULE, NextClaim, NUMBER_OF_STAKED_NFTS}, ContractError, msg::{UpdateHistoriesResponse}};

pub const IS_STAKED: bool = true;
const MIN_CYCLE_LENGTH: u64 = 10;
const MIN_PERIOD: u64 = 2;


// convert string to Addr type.
pub fn from_string_to_addr(
    deps: DepsMut,
    addr_string: String
) -> StdResult<Addr> {
    let return_addr = deps.api.addr_validate(&addr_string).unwrap();
    Ok(return_addr)
}

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
            cycle_length_in_seconds: cycle_length_in_seconds 
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
            period_length_in_cycles: period_length_in_cycles 
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
        funds: [Coin{
            denom: "".to_string(),
            amount: Uint128::from_str("0").unwrap(),
        }].to_vec(),
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
pub fn check_contract_owner(
    info: MessageInfo,
    config: Config,
) -> Result<bool, ContractError> {
    if config.owner != info.sender{
        return Err(ContractError::Unauthorized {});
    }

    Ok(true)
}

// check message sender is nft owner which records in the TOKEN_INFOs state.
pub fn check_staker(
    deps: DepsMut,
    info: MessageInfo,
    token_id: String,
) -> Result<TokenInfo, ContractError> {

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
            recipient: recipient, 
            amount:  u128_amount,
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
) -> Result<Vec<CosmosMsg>, ContractError> {
    let mut messages: Vec<CosmosMsg> = vec![];

    let transfer_from: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: nft_contract,
        msg: to_binary(&Cw721ExecuteMsg::TransferNft { 
            recipient: staker, 
            token_id: token_id, 
        })?,
        funds: vec![]
    });
    messages.push(transfer_from);

    Ok(messages)
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
            address: address 
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
) -> Result<UpdateHistoriesResponse, ContractError> {
    let staker_snapshot_index = update_staker_history(deps.branch(), is_staked, current_cycle, staker_tokenid_key.clone())?;
    let staker_history = STAKER_HISTORIES.may_load(deps.branch().storage, staker_tokenid_key.clone()).unwrap().unwrap();
    let staker_snapshot = &staker_history[staker_snapshot_index as usize];

    let update_histories_res = UpdateHistoriesResponse {
        staker: staker_tokenid_key,
        current_cycle: current_cycle,
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
                is_staked: is_staked,
                start_cycle: current_cycle,
            };

            staker_history.clone().append(&mut vec![new_snapshot]);
            STAKER_HISTORIES.save(deps.storage, staker_tokenid_key, &staker_history)?;

            return Ok(snapshot_index)
        } 
    }

    let new_snapshot = Snapshot {
        is_staked: is_staked,
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
    max_period: u64,
    now: u64,
    start_timestamp: u64,
    config: Config,
) -> Result<(Claim, NextClaim), ContractError> {
    let mut claim = Claim {
        start_period: 0,
        periods: 0,
        amount: 0,
    };

    let mut next_claim = NextClaim {
        period: 0,
        staker_snapshot_index: 0,
    };

    // computing 0 periods.
    if max_period == 0 {
        return Ok((claim, next_claim))
    }

    next_claim = NEXT_CLAIMS.may_load(deps.storage, staker_tokenid_key.clone()).unwrap().unwrap();
    claim.start_period = next_claim.period;

    // nothing has been staked yet.
    if claim.start_period == 0 {
        return Ok((claim, next_claim))
    }

    let end_claim_period = get_current_period(now, start_timestamp, config.clone()).unwrap();

    // current period is not claimable.
    if next_claim.period == end_claim_period {
        return Ok((claim, next_claim))
    }

    // retrieve the next snapshots if they exist.
    let staker_history = STAKER_HISTORIES.may_load(deps.storage, staker_tokenid_key.clone()).unwrap().unwrap();

    let s_state_data = staker_history[next_claim.clone().staker_snapshot_index as usize].clone();
    let mut staker_snapshot = Snapshot {
        is_staked: s_state_data.is_staked,
        start_cycle: s_state_data.start_cycle,
    };

    let mut next_staker_snapshot = snapshot_init().unwrap();
    if next_claim.staker_snapshot_index != staker_history.clone().len() as u64 - 1 {
        let s_data = &staker_history.clone()[(next_claim.staker_snapshot_index + 1) as usize];
        next_staker_snapshot = Snapshot {
            is_staked: s_data.is_staked,
            start_cycle: s_data.start_cycle,
        }
    }

    // exclues the current period.
    claim.periods = end_claim_period - next_claim.period;
    if max_period < claim.periods {
        claim.periods = max_period;
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
                    next_staker_snapshot = Snapshot {
                        is_staked: false,
                        start_cycle: 0,
                    }
                }
            } 
        }
        next_claim.period = next_claim.period + 1;   
    }

    Ok((claim, next_claim))

}

// snapshot init
pub fn snapshot_init() -> Result<Snapshot, ContractError> {
    let snapshot = Snapshot {
        is_staked: false,
        start_cycle: 0,
    };
    Ok(snapshot)
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