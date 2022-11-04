#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;
use cw20::{Cw20ReceiveMsg};
use cw721::Cw721ReceiveMsg;

use crate::error::{ContractError};
use crate::handler::{execute_token_contract_transfer, get_cycle, get_period, update_histories, IS_STAKED, check_start_timestamp, check_disable, check_contract_owner, execute_transfer_nft_unstake, check_staker, compute_rewards, staker_tokenid_key, get_current_period, query_rewards_token_balance, is_valid_cycle_length, is_valid_period_length, contract_info};
use crate::msg::{ExecuteMsg, InstantiateMsg, SetConfigMsg};
use crate::state::{Config, CONFIG_STATE, START_TIMESTAMP, REWARDS_SCHEDULE, TOTAL_REWARDS_POOL, DISABLE, NEXT_CLAIMS, NextClaim, TOKEN_INFOS, TokenInfo, STAKER_HISTORIES, Claim};

// version info for migration info
const CONTRACT_NAME: &str = "nft-staking";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    is_valid_cycle_length(msg.cycle_length_in_seconds)?;
    is_valid_period_length(msg.period_length_in_cycles)?;

    // setup contract configuration.
    // the owner is contract instantiater and is able to execute functions except stake, unstake and claim rewards.
    // Warning: cycles and periods need to be calibrated carefully. 
    //          Small values will increase computation load while estimating and claiming rewards. 
    //          Big values will increase the time to wait before a new period becomes claimable.
    // rewards_token_contract is cw20 and white_listed_nft_contract is cw721.
    let config_state = Config {
        owner: info.sender.clone(),
        cycle_length_in_seconds: msg.cycle_length_in_seconds,
        period_length_in_cycles: msg.period_length_in_cycles,
        white_listed_nft_contract: msg.white_listed_nft_contract,
        rewards_token_contract: msg.rewards_token_contract,
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    CONFIG_STATE.save(deps.storage, &config_state)?;

    // Default of total rewards pool is zero and of disable state is false.
    TOTAL_REWARDS_POOL.save(deps.storage, &0)?;
    DISABLE.save(deps.storage, &false)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("contract_owner", config_state.owner)
        .add_attribute("cycle_length_in_seconds", config_state.cycle_length_in_seconds.to_string())
        .add_attribute("period_length_in_cycles", config_state.period_length_in_cycles.to_string())
        .add_attribute("white_listed_nft_contract", config_state.white_listed_nft_contract)
        .add_attribute("reward_token_contract", config_state.rewards_token_contract)
    )        
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG_STATE.load(deps.storage)?;
    
    match msg {
        ExecuteMsg::SetConfig(msg) => set_config(deps, info, config, msg),
        ExecuteMsg::AddRewardsForPeriods { rewards_per_cycle } => add_rewards_for_periods(deps, env, info, rewards_per_cycle, config),
        ExecuteMsg::Receive (msg) => add_rewards_pool(deps, info, env, config, msg),
        ExecuteMsg::Start {} => start(deps, info, env, config),
        ExecuteMsg::Disable {} => disable(deps, info, config),
        ExecuteMsg::Enable {} => enable(deps, info, config),
        ExecuteMsg::WithdrawRewardsPool { amount } => withdraw_rewards_pool(deps, info, config, amount),
        ExecuteMsg::WithdrawAllRewardsPool {} => withdraw_all_rewards_pool(deps, info, env, config),
        ExecuteMsg::ReceiveNft(msg) => stake_nft(deps, env, info, config, msg),
        ExecuteMsg::UnstakeNft { token_id, staker, claim_recipient_address } => unstake_nft(deps, env, info, config, token_id, staker, claim_recipient_address),
        ExecuteMsg::ClaimRewards { max_period, token_id, claim_recipient_address } => claim_rewards(deps, info, env, max_period, token_id, config, claim_recipient_address),
    }
}

// change configuration.
pub fn set_config(
    deps: DepsMut,
    info: MessageInfo,
    config: Config,
    msg: SetConfigMsg,
) -> Result<Response, ContractError> {
    check_contract_owner(info.clone(), config.clone())?;

    let mut cycle_length_in_seconds = config.clone().cycle_length_in_seconds;
    let mut period_length_in_cycles = config.clone().period_length_in_cycles;
    let mut white_listed_nft_contract = config.clone().white_listed_nft_contract;
    let mut rewards_token_contract = config.clone().rewards_token_contract;

    if !msg.cycle_length_in_seconds.is_none() && is_valid_cycle_length(msg.cycle_length_in_seconds.unwrap())? {
        cycle_length_in_seconds = msg.cycle_length_in_seconds.unwrap();
    } 
    if !msg.period_length_in_cycles.is_none() && is_valid_period_length(msg.period_length_in_cycles.unwrap())? {
        period_length_in_cycles = msg.period_length_in_cycles.unwrap();
    }
    if !msg.white_listed_nft_contract.is_none() {
        white_listed_nft_contract = msg.white_listed_nft_contract.unwrap();
    }
    if !msg.rewards_token_contract.is_none() {
        rewards_token_contract = msg.rewards_token_contract.unwrap();
    }

    let config_state = Config {
        owner: config.clone().owner,
        cycle_length_in_seconds: cycle_length_in_seconds.clone(),
        period_length_in_cycles: period_length_in_cycles.clone(),
        white_listed_nft_contract: white_listed_nft_contract.clone(),
        rewards_token_contract: rewards_token_contract.clone(),
    };

    CONFIG_STATE.save(deps.storage, &config_state)?;

    Ok(Response::new()
        .add_attribute("method", "set_config")
        .add_attribute("new_cycle_length_in_seconds", cycle_length_in_seconds.to_string())
        .add_attribute("new_period_length_in_cycles", period_length_in_cycles.to_string())
        .add_attribute("new_white_listed_nft_contract", white_listed_nft_contract)
        .add_attribute("new_rewards_token_contract", rewards_token_contract)
    )
}

// set rewards schedule.
// rewards per cycle can changed by executing add_rewards_for_periods even after start.
// if rewards per cycle are replaced to new value of rewards per cycle, 
// computing rewards are changed immediatly when staker claims rewards. 
pub fn add_rewards_for_periods(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    rewards_per_cycle: u128,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(info.clone(), config.clone())?;

    // rewards per cycle shoule be bigger than zero.
    if rewards_per_cycle <= 0 {
        return Err(ContractError::InvalidRewardsSchedule {})
    }
    REWARDS_SCHEDULE.save(deps.storage, &rewards_per_cycle)?;
    
    Ok(Response::new()
        .add_attribute("method", "add_rewards_for_periods")
        .add_attribute("rewards_per_cycle", rewards_per_cycle.to_string())
    )
}

// increase rewards pool.
// nft staking contract requests to transfer rewards from contract instantiater, as contract owner, to nft staking contract.
pub fn add_rewards_pool (
    deps: DepsMut,
    info: MessageInfo,
    _env: Env,
    config: Config,
    msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    if info.sender.to_string() != config.clone().rewards_token_contract {
        return Err(ContractError::InvalidRewardsTokenContract { 
            rewards_token_contract: config.clone().rewards_token_contract, 
            requester: info.sender.to_string(), 
        })
    }

    check_contract_owner(contract_info(msg.clone()).unwrap(), config.clone())?;

    let total_rewards_pool = TOTAL_REWARDS_POOL.load(deps.storage)?;
    let rewards = total_rewards_pool + msg.amount.clone().u128();

    TOTAL_REWARDS_POOL.save(deps.storage, &rewards)?;

    Ok(Response::new()
        .add_attribute("method", "add_rewards_pool")
        .add_attribute("added_rewards", msg.amount.to_string())
        .add_attribute("total_rewards", rewards.to_string())
    )
}

// nft staking contract start.
// every calculating period and cycle are affected by start timestamp.
pub fn start(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(info.clone(), config.clone())?;

    let start_timestamp = START_TIMESTAMP.may_load(deps.storage)?;
    if !start_timestamp.is_none() {
        return Err(ContractError::AlreadyStarted {})
    }
    let now = env.block.time.seconds();
    
    START_TIMESTAMP.save(deps.storage, &now)?;

    Ok(Response::new()
        .add_attribute("method", "start")
        .add_attribute("start_time_stamp", now.to_string())
    )
}

// nft staking contract halt.
// after disabled, functions are stop.
pub fn disable(
    deps: DepsMut,
    info: MessageInfo,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(info.clone(), config.clone())?;

    DISABLE.save(deps.storage, &true)?;

    Ok(Response::new()
        .add_attribute("method", "disable")
        .add_attribute("disable", true.to_string())
    )
}

// if the nft staking contract is disabled and the contract owner want to activate again, 
// execute enable function.
pub fn enable(
    deps: DepsMut,
    info: MessageInfo,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(info.clone(), config.clone())?;

    let disable = DISABLE.load(deps.storage)?;
    if !disable {
        return Err(ContractError::CannotEnable { disable: disable })
    }

    DISABLE.save(deps.storage, &!disable)?;

    Ok(Response::new()
        .add_attribute("method", "enable")
        .add_attribute("previous_disable_state", disable.to_string())
        .add_attribute("now_disable_state", (!disable).to_string())
    )
}

// withdraw rewards pool.
// the nft staking contract's balance of token rewards which is value of requested amount transfer to contract owner.
pub fn withdraw_rewards_pool(
    deps: DepsMut,
    info: MessageInfo,
    config: Config,
    amount: u128,
) -> Result<Response, ContractError> {
    check_contract_owner(info.clone(), config.clone())?;

    let disabled = check_disable(deps)?;
    let rewards_token_contract = config.clone().rewards_token_contract;
    let owner = info.clone().sender;
    let message = execute_token_contract_transfer(rewards_token_contract, owner.to_string(), amount.clone())?;

    Ok(Response::new()
        .add_attribute("method", "withdraw_rewards_pool")
        .add_attribute("disable", disabled.to_string())
        .add_attribute("rewards_token_contract", config.rewards_token_contract)
        .add_attribute("owner", info.sender.to_string())
        .add_attribute("withdraw_amount", amount.to_string())
        .add_messages(message)
    )
}

// withdraw all rewards pool.
// the nft staking contract's all balances transfer to contract owner.
pub fn withdraw_all_rewards_pool(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(info.clone(), config.clone())?;

    let disabled = check_disable(deps.branch())?;
    let rewards_token_contract = config.clone().rewards_token_contract;
    let owner = info.clone().sender;
    let address = env.contract.address.to_string();

    // nft staking contract balances
    let balance_response = query_rewards_token_balance(deps.as_ref(), address.clone(), rewards_token_contract.clone())?;
    let amount = balance_response.balance.u128();

    let message = execute_token_contract_transfer(rewards_token_contract, owner.to_string(), amount.clone())?;

    Ok(Response::new()
        .add_attribute("method", "withdraw_all_rewards_pool")
        .add_attribute("disable", disabled.to_string())
        .add_attribute("rewards_token_contract", config.rewards_token_contract)
        .add_attribute("nft_staking_contract", address)
        .add_attribute("owner", info.sender.to_string())
        .add_attribute("withdraw_amount", amount.to_string())
        .add_messages(message)
    )
}

// staking nft.
// the staker can stake nft as cw721.
pub fn stake_nft(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    config: Config,
    msg: Cw721ReceiveMsg,
) -> Result<Response, ContractError> {
    let total_rewards_pool = TOTAL_REWARDS_POOL.may_load(deps.branch().storage)?;
    if total_rewards_pool.is_none() {
        return Err(ContractError::EmptyRewardsPool {})
    }

    let rewards_schedule = REWARDS_SCHEDULE.may_load(deps.branch().storage)?;
    if rewards_schedule.is_none() {
        return Err(ContractError::NoneRewardsSchedule {})
    }

    if info.sender.to_string() != config.clone().white_listed_nft_contract {
        return Err(ContractError::InvalidWhitelistedContract { 
            white_listed_contract: config.clone().white_listed_nft_contract, 
            requester: info.sender.to_string() 
        })
    }

    let start_timestamp = check_start_timestamp(deps.branch())?;
    check_disable(deps.branch())?;

    let staker = msg.sender;
    let token_id = msg.token_id;
    let send_nft_msg = msg.msg;
    let timestamp = env.block.time.seconds();
    let current_cycle = get_cycle(timestamp, start_timestamp, config.clone())?;

    // the nft for staking is managed by mapping staker address and nft token ID.
    // the staker that stakes multi nft can claim rewards for each nft.
    let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());

    let update_histories_response = update_histories(deps.branch(), staker_tokenid_key.clone(), IS_STAKED, current_cycle)?;
    let next_claims = NEXT_CLAIMS.may_load(deps.branch().storage, staker_tokenid_key.clone()).unwrap();

    // initialise the next claim if it was the first stake for this staker or if 
    // the next claim was re-initialised.
    // i.e. rewards were claimed until the last staker snapshot and the last staker snapshot is not staked.
    if next_claims.is_none() {
        let current_period = get_period(current_cycle, config.clone())?;
        let new_next_claim = NextClaim {
            period: current_period,
            staker_snapshot_index: 0,
        };

        NEXT_CLAIMS.save(deps.branch().storage, staker_tokenid_key.clone(), &new_next_claim)?;
    }

    let token_infos = TOKEN_INFOS.may_load(deps.branch().storage, token_id.clone()).unwrap();
    if !token_infos.is_none() {
        let withdraw_cycle = token_infos.unwrap().withdraw_cycle;

        // cannot re-stake when current cycle of block time is same setup withdraw cycle
        if current_cycle == withdraw_cycle {
            return Err(ContractError::UnstakedTokenCooldown {})
        }    
    }

    let new_token_info = TokenInfo {
        owner: staker.clone(),
        is_staked: IS_STAKED,
        deposit_cycle: current_cycle,
        withdraw_cycle: 0,
    };
    TOKEN_INFOS.save(deps.branch().storage, token_id.clone(), &new_token_info)?;

    Ok(Response::new()
        .add_attribute("method", "stake_nft")
        .add_attribute("nft_owner", staker)
        .add_attribute("current_cycle", current_cycle.to_string())
        .add_attribute("staker_histories_stake", update_histories_response.staker_histories_stake.to_string())
        .add_attribute("nft_exist", new_token_info.is_staked.to_string())
        .add_attribute("send_nft_message", send_nft_msg.to_string())
    )
}

// unstaking nft
// the staker can unbond the nft as cw721.
pub fn unstake_nft(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    config: Config,
    token_id: String,
    staker: String,
    claim_recipient_address: Option<String>,
) -> Result<Response, ContractError> {
    let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());
    let token_info = check_staker(deps.branch(), info.clone(), token_id.clone())?;

    let start_timestamp = check_start_timestamp(deps.branch())?;
    let timestamp = env.block.time.seconds();

    let current_cycle = get_cycle(timestamp, start_timestamp, config.clone())?;
    let is_staked = token_info.clone().is_staked;

    let disable = check_disable(deps.branch())?;
    let mut claim_rewards_response = Response::new();
    if !disable {
        // ensure that at least an entire cycle has elapsed before unstaking the token to avoid
        // an exploit where a full cycle would be claimable if staking just before the end
        // of a cycle and unstaking right after start of the new cycle.
        if !(current_cycle - token_info.clone().deposit_cycle >= 2) {
            return Err(ContractError::TokenSteelFrozen {})
        }

        // before unstake the nft by staker, rewards token balances are transfer to staker.
        let current_period = get_current_period(timestamp.clone(), start_timestamp.clone(), config.clone()).unwrap();
        let compute_rewards = compute_rewards(deps.as_ref(), staker_tokenid_key.clone(), current_period, timestamp, start_timestamp, config.clone()).unwrap();
        if compute_rewards.0.amount != 0 {
            let mut recipient: Option<String> = None;
            if !claim_recipient_address.is_none() {
                recipient = claim_recipient_address;
            }
            claim_rewards_response = claim_rewards(deps.branch(), info, env, current_period, token_id.clone(), config.clone(), recipient).unwrap();
        }

        update_histories(deps.branch(), staker_tokenid_key.clone(), !is_staked, current_cycle)?;

        // clear the token owner to ensure it cannot be unstaked again without being re-staked.
        // set the withdrawal cycle to ensure it cannot be re-staked during the same cycle.
        let token_info = TokenInfo {
            owner: "".to_string(),
            is_staked: !is_staked,
            withdraw_cycle: current_cycle,
            deposit_cycle: token_info.clone().deposit_cycle,            
        };

        TOKEN_INFOS.save(deps.branch().storage, token_id.clone(), &token_info)?;
    }

    // next claims of specified nft are eliminated.
    NEXT_CLAIMS.remove(deps.branch().storage, staker_tokenid_key.clone());

    let message = execute_transfer_nft_unstake(token_id, staker, config.white_listed_nft_contract)?;

    Ok(Response::new()
        .add_attribute("method", "unstake_nft")
        .add_attribute("request_unstake_time", timestamp.to_string())
        .add_attributes(claim_rewards_response.attributes)
        .add_submessages(claim_rewards_response.messages)
        .add_messages(message)
    )
}

// claim rewards are generated by staking the nft.
// claims the claimable rewards for the specified max number of past periods, starting at the next claimable period.
// claims can be done only for periods which have already ended.
// the max number of periods to claim can be calibrated to chunk down claims in several transactions to accomodate gas constraints.
pub fn claim_rewards(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    max_period: u64,
    token_id: String,
    config: Config,
    claim_recipient_address: Option<String>,
) -> Result<Response, ContractError> {
    let start_timestamp = check_start_timestamp(deps.branch())?;
    check_disable(deps.branch())?;

    let staker = info.clone().sender.to_string();
    let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());

    let next_claim = NEXT_CLAIMS.may_load(deps.storage, staker_tokenid_key.clone())?;
    if next_claim.is_none() {
        return Err(ContractError::EmptyNextClaim {})
    }
    let next_claim = next_claim.unwrap();

    let now = env.block.time.seconds();
    let claim: Claim;
    let new_next_claim: NextClaim;
    let compute_rewards = compute_rewards(deps.as_ref(), staker_tokenid_key.clone(), max_period, now, start_timestamp, config.clone());
    match compute_rewards {
        Ok(t) => {
            claim = t.0;
            new_next_claim = t.1;
        },
        Err(e) => {
            return Err(e)
        }
    }

    // free up memory on already processed staker snapshots.
    let staker_history = STAKER_HISTORIES.may_load(deps.storage, staker_tokenid_key.clone())?;
    if staker_history.is_none() {
        return Err(ContractError::HaveNotHistory {})
    }
    let mut staker_history = staker_history.unwrap();
    while next_claim.staker_snapshot_index < new_next_claim.staker_snapshot_index {
        let delete_index = next_claim.staker_snapshot_index + 1;
        staker_history.remove(delete_index as usize);
        STAKER_HISTORIES.save(deps.storage, staker_tokenid_key.clone(), &staker_history)?;
    }

    if claim.periods == 0 || next_claim.period == 0{
        return Err(ContractError::InvalidClaim {})
    }

    let last_staker_snapshot = staker_history[(staker_history.len() - 1) as usize];
    let last_claimed_cycle = (claim.start_period + claim.periods - 1) * config.period_length_in_cycles;

    // the claim reached the last staker snapshot and nothing is staked in the last staker snapshot.
    if last_claimed_cycle >= last_staker_snapshot.start_cycle && last_staker_snapshot.is_staked == false {
        
        // re-init the next claim.
        NEXT_CLAIMS.remove(deps.storage, staker_tokenid_key.clone());
    } else {
        NEXT_CLAIMS.save(deps.storage, staker_tokenid_key.clone(), &new_next_claim)?;
    }

    if claim.amount == 0 {
        return Err(ContractError::NoAmountClaim {})
    }
    
    // if staker want to transfer send other address as request claim function, set claim recipient address. 
    let mut recipient = staker;
    if !claim_recipient_address.is_none() {
        recipient = claim_recipient_address.unwrap();
    }

    // transfer token amount of staked rewards.
    let message = execute_token_contract_transfer(config.rewards_token_contract, recipient.clone(), claim.amount);
    match message {
        Ok(_) => {},
        Err(e) => {
            return Err(e)
        }
    }

    Ok(Response::new()
        .add_attribute("method", "claim_rewards")
        .add_attribute("claim_start_period", claim.start_period.to_string())
        .add_attribute("claim_periods", claim.periods.to_string())
        .add_attribute("claim_amount", claim.amount.to_string())
        .add_attribute("claim_recipient", recipient.to_string())
        .add_messages(message.unwrap())
    )
}

#[cfg(test)]
mod tests{
    use std::ops::Add;
    use std::str::FromStr;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MockApi, MockQuerier};
    use cosmwasm_std:: {MessageInfo, DepsMut, Env, Empty, MemoryStorage, OwnedDeps, Addr, Uint128, BlockInfo, Timestamp, TransactionInfo, ContractInfo, to_binary, Response, Binary};
    use cw20::{Cw20Coin, MinterResponse, Cw20ReceiveMsg, Cw20ExecuteMsg};
    use cw20_base::contract::{instantiate, execute};
    use cw721::{Cw721ReceiveMsg, Cw721Execute};
    use cw721_base::{Cw721Contract, Extension, InstantiateMsg as Cw721BaseInstantiateMsg, ExecuteMsg as Cw721BaseExecuteMsg, MintMsg};
    use cw20_base::{msg::InstantiateMsg as Cw20InstantiateMsg};
    use crate::execute::add_rewards_pool;
    use crate::handler::{get_cycle, update_histories, IS_STAKED, get_period, check_start_timestamp, check_disable, staker_tokenid_key, get_current_period, snapshot_init, execute_token_contract_transfer};
    use crate::state::{Config, CONFIG_STATE, TOTAL_REWARDS_POOL, REWARDS_SCHEDULE, DISABLE, NEXT_CLAIMS, NextClaim, TOKEN_INFOS, TokenInfo, Claim, STAKER_HISTORIES, Snapshot};
    use crate::error::ContractError;

    use super::{add_rewards_for_periods, start};

    const CONTRACT_NAME: &str = "CW721CTRT";
    const SYMBOL: &str = "CW721";

    fn set_config (
        deps: DepsMut,
        info: MessageInfo,
        cw721_contract: String,
        cw20_contract: String,
    ) {
        let cw721 = cw721_contract;
        let cw20 = cw20_contract;
        let config_state = Config {
            owner: info.sender.clone(),
            cycle_length_in_seconds: 60,
            period_length_in_cycles: 3,
            white_listed_nft_contract: cw721,
            rewards_token_contract: cw20.to_string(),
        };
    
        CONFIG_STATE.save(deps.storage, &config_state).unwrap();
        TOTAL_REWARDS_POOL.save(deps.storage, &0).unwrap();
        DISABLE.save(deps.storage, &false).unwrap();
    }

    fn get_config(
        deps: DepsMut,
    ) -> Result<Config, ()>{
        let config = CONFIG_STATE.load(deps.storage).unwrap();

        Ok(config)
    }

    fn test_environment()
    ->  (
            OwnedDeps<MemoryStorage, MockApi, MockQuerier>,
            MessageInfo,
            Env,
            Cw721Contract<'static, Extension, Empty, Empty, Empty>,
            Addr,
            Config,
            String,
            String,
        ) {
        let minter = String::from("xpla1j55tymfdys9n7k0dq6xmyd4hgfelp9jghzympt");

        let staker = "xpla1ma4peq833n2k3t7u2f60w420ltx8nvz0g0vwlu".to_string();
        let token_id = "token_id_test_0".to_string();

        let mut deps = mock_dependencies();
        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();

        let cw721_contract = setup_contract(deps.as_mut());
        let cw721_contract_address = mock_env().contract.address;

        let mint_msg = Cw721BaseExecuteMsg::Mint(MintMsg::<Extension> {
            token_id: token_id.clone(),
            owner: staker.clone(),
            token_uri: None,
            extension: None,
        });
        cw721_contract
            .execute(deps.as_mut(), mock_env(), info.clone(), mint_msg)
            .unwrap();

        setup_contract_cw20(deps.as_mut());
        let cw20_contract_address = mock_env_cw20().contract.address;

        // set config of nft staking contract
        set_config(deps.as_mut(), info.clone(), cw721_contract_address.to_string(), cw20_contract_address.to_string());
        let config = get_config(deps.as_mut()).unwrap();

        let rewards_per_cycle: u128 = 17;
        let add_rewards_schedule = add_rewards_for_periods(deps.as_mut(), env.clone(), info.clone(), rewards_per_cycle, config.clone()).unwrap();
        
        // method
        assert_eq!(add_rewards_schedule.attributes.get(0).unwrap().value, "add_rewards_for_periods");
        // rewards_per_cycle
        assert_eq!(add_rewards_schedule.attributes.get(1).unwrap().value, "17");

        let add_rewards = Uint128::from_str("2000000000").unwrap();
        let send_msg = Binary::from(r#"{add_rewards}"#.as_bytes());

        let msg = Cw20ExecuteMsg::Send {
            contract: env.contract.address.clone().to_string(),
            amount: add_rewards.clone(),
            msg: send_msg.clone(),
        };
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
        assert_eq!(res.messages.len(), 1);

        let msg = Cw20ReceiveMsg {
            sender: minter.clone(),
            amount: add_rewards.clone(),
            msg: send_msg.clone()
        };
        let cw20_info = mock_info(cw20_contract_address.as_str(), &[]);
        let add_rewards_pool = add_rewards_pool(deps.as_mut(), cw20_info.clone(), env.clone(), config.clone(), msg).unwrap();

        // method
        assert_eq!(add_rewards_pool.attributes.get(0).unwrap().value, "add_rewards_pool");        
        // added_rewards
        assert_eq!(add_rewards_pool.attributes.get(1).unwrap().value, "2000000000");
        // total_rewards
        assert_eq!(add_rewards_pool.attributes.get(2).unwrap().value, "2000000000");

        contract_test_start(deps.as_mut(), info.clone(), env.clone(), config.clone());

        return (deps, info, env, cw721_contract, cw721_contract_address, config, staker, token_id)
    }

    fn contract_test_start(
        deps: DepsMut,
        info: MessageInfo,
        env: Env,
        config: Config,
    ) {
        start(deps, info, env, config).unwrap();
    }

    fn setup_contract(deps: DepsMut<'_>) -> Cw721Contract<'static, Extension, Empty, Empty, Empty> {
        let contract = Cw721Contract::default();
        let msg = Cw721BaseInstantiateMsg {
            name: CONTRACT_NAME.to_string(),
            symbol: SYMBOL.to_string(),
            minter: String::from("xpla1j55tymfdys9n7k0dq6xmyd4hgfelp9jghzympt"),
        };
        let info = mock_info("creator", &[]);
        let res = contract.instantiate(deps, mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        contract
    }

    fn setup_contract_cw20(deps: DepsMut<'_>) {
        let addr = String::from("xpla1j55tymfdys9n7k0dq6xmyd4hgfelp9jghzympt");
        let amount = Uint128::new(200000000000);
        let minter = String::from("xpla1j55tymfdys9n7k0dq6xmyd4hgfelp9jghzympt");
        let limit = Uint128::new(400000000000);

        let instantiate_msg = Cw20InstantiateMsg {
            name: "Auto Gen".to_string(),
            symbol: "AUTO".to_string(),
            decimals: 18,

            initial_balances: vec![Cw20Coin {
                address: addr.to_string(),
                amount,
            }],
            mint: Some(MinterResponse {
                minter: minter.to_string(),
                cap: Some(limit),
            }),
            marketing: None,
        };
        
        let info = mock_info("creator", &[]);
        let env = mock_env_cw20();
        let res = instantiate(deps, env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

    }

    pub fn mock_env_cw20() -> Env {
        Env {
            block: BlockInfo {
                height: 12_345,
                time: Timestamp::from_nanos(1_571_797_419_879_305_533),
                chain_id: "cosmos-testnet-14002".to_string(),
            },
            transaction: Some(TransactionInfo { index: 3 }),
            contract: ContractInfo {
                address: Addr::unchecked("cosmos2contract_cw20"),
            },
        }
    }

    #[test]
    fn test_add_rewards_for_periods() {
        let minter = String::from("xpla1j55tymfdys9n7k0dq6xmyd4hgfelp9jghzympt");

        let mut deps = mock_dependencies();
        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();
        let cw20_env = mock_env_cw20();

        set_config(deps.as_mut(), info.clone(), env.contract.address.to_string(), cw20_env.contract.address.to_string());
        let rewards_per_cycle: u128 = 17;

        REWARDS_SCHEDULE.save(deps.as_mut().storage, &rewards_per_cycle).unwrap();
    }

    #[test]
    fn test_stake() {
        // test environment
        let (mut deps, info, env, cw721_contract, cw721_contract_address, config, staker, token_id) = test_environment();
        let staker_info = mock_info(staker.as_str(), &[]);
        let msg = to_binary( "send nft to stake").unwrap();
        let res = cw721_contract.send_nft(deps.as_mut(), env.clone(), staker_info.clone(), env.contract.address.clone().to_string(), token_id.clone(), msg.clone()).unwrap();

        let payload = Cw721ReceiveMsg {
            sender: staker.to_string(),
            token_id: token_id.clone(),
            msg,
        };
        let expected = payload.into_cosmos_msg(env.contract.address.clone()).unwrap();
        assert_eq!(
            res,
            Response::new()
                .add_message(expected)
                .add_attribute("action", "send_nft")
                .add_attribute("sender", staker.to_string())
                .add_attribute("recipient", env.contract.address.to_string())
                .add_attribute("token_id", token_id.clone())
        );

        let timestamp = env.block.time.seconds();
        stake_function(deps.as_mut(), info, env, timestamp, cw721_contract_address, config, staker, token_id);
    }


    fn stake_function(
        mut deps: DepsMut,
        _info: MessageInfo,
        _env: Env,
        timestamp: u64,
        _cw721_contract_address: Addr,
        config: Config,
        staker: String,
        token_id: String,
    ) {
        let start_timestamp = check_start_timestamp(deps.branch()).unwrap();
        
        check_disable(deps.branch()).unwrap();

        let current_cycle = get_cycle(timestamp, start_timestamp, config.clone()).unwrap();
        let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());

        let update_histories_response = update_histories(deps.branch(), staker_tokenid_key.clone(), IS_STAKED, current_cycle).unwrap();
        assert_eq!(update_histories_response.staker, "xpla1ma4peq833n2k3t7u2f60w420ltx8nvz0g0vwlu@token_id_test_0");
        assert_eq!(update_histories_response.staker_histories_stake, true);

        let next_claims = NEXT_CLAIMS.may_load(deps.branch().storage, staker_tokenid_key.clone()).unwrap();

        if next_claims.is_none() {
            let current_period = get_period(current_cycle, config.clone()).unwrap();
            let new_next_claim = NextClaim {
                period: current_period,
                staker_snapshot_index: 0,
            };

            NEXT_CLAIMS.save(deps.branch().storage, staker_tokenid_key.clone(), &new_next_claim).unwrap();
        }

        let token_infos = TOKEN_INFOS.may_load(deps.branch().storage, token_id.clone()).unwrap();
        if !token_infos.is_none(){
            let withdraw_cycle = token_infos.unwrap().withdraw_cycle;
            
            assert_ne!(current_cycle, withdraw_cycle);
            if current_cycle == withdraw_cycle {
                println!("{:?}", ContractError::UnstakedTokenCooldown {}.to_string());
            }    
        }

        let new_token_info = TokenInfo {
            owner: staker.clone(),
            is_staked: IS_STAKED,
            deposit_cycle: current_cycle,
            withdraw_cycle: 0,
        };
        TOKEN_INFOS.save(deps.branch().storage, token_id.clone(), &new_token_info).unwrap();
    }

    #[test]
    fn test_unstake () {
        let (mut deps, info, env, cw721_contract,cw721_contract_address, config, staker, token_id) = test_environment();

        let staker_info = mock_info(staker.as_str(), &[]);
        let msg = to_binary( "send nft to stake").unwrap();
        cw721_contract.send_nft(deps.as_mut(), env.clone(), staker_info.clone(), env.contract.address.clone().to_string(), token_id.clone(), msg.clone()).unwrap();
        
        let timestamp = env.block.time.seconds();
        stake_function(deps.as_mut(), info.clone(), env.clone(), timestamp.clone(), cw721_contract_address.clone(), config.clone(), staker.clone(), token_id.clone());

        let timestamp = timestamp + 2000;
        unstake_function(deps.as_mut(), info, env, cw721_contract_address, config, staker, token_id, timestamp);
    }

    fn unstake_function(
        mut deps: DepsMut,
        _info: MessageInfo,
        env: Env,
        _cw721_contract_address: Addr,
        config: Config,
        staker: String,
        token_id: String,
        timestamp: u64,
    ) {
        // unstake test
        let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());

        let token_infos = TOKEN_INFOS.may_load(deps.branch().storage, token_id.clone()).unwrap();
        assert_eq!(token_infos.is_none(), false);
        assert_eq!(token_infos.clone().unwrap().owner, staker);

        let start_timestamp = check_start_timestamp(deps.branch()).unwrap();
        let current_cycle = get_cycle(timestamp, start_timestamp, config.clone()).unwrap();
        let is_staked = token_infos.clone().unwrap().is_staked;

        let disable = check_disable(deps.branch()).unwrap();
        if !disable {
            assert!((current_cycle - token_infos.clone().unwrap().deposit_cycle >= 2));

            let current_period = get_current_period(timestamp.clone(), start_timestamp.clone(), config.clone()).unwrap();
            let compute_rewards = compute_rewards_function(deps.branch(), staker_tokenid_key.clone(), current_period, timestamp, start_timestamp, config.clone());

            if compute_rewards.0.amount != 0 {
                let info = mock_info(staker.as_ref(), &[]);
                claim_rewards_function(deps.branch(), info, env, current_period, token_id.clone(), config.clone(), timestamp);
            }

            let update_histories_response = update_histories(deps.branch(), staker_tokenid_key.clone(), !is_staked, current_cycle).unwrap();
            assert_eq!(update_histories_response.staker, "xpla1ma4peq833n2k3t7u2f60w420ltx8nvz0g0vwlu@token_id_test_0");
            assert_eq!(update_histories_response.staker_histories_stake, false);

            let token_info = TokenInfo {
                owner: "0".to_string(),
                is_staked: !is_staked,
                withdraw_cycle: current_cycle,
                deposit_cycle: token_infos.clone().unwrap().deposit_cycle,            
            };

            TOKEN_INFOS.save(deps.branch().storage, token_id.clone(), &token_info).unwrap();
        }

        NEXT_CLAIMS.remove(deps.branch().storage, staker_tokenid_key.clone());
    }

    #[test]
    fn test_compute_rewards() {
        let (mut deps, info, env, cw721_contract, cw721_contract_address, config, staker, token_id) = test_environment();
        
        let staker_info = mock_info(staker.as_str(), &[]);
        let contract_info = mock_info(cw721_contract_address.as_str(), &[]);
        let msg = to_binary( "send nft to stake").unwrap();
        cw721_contract.send_nft(deps.as_mut(), env.clone(), staker_info.clone(), env.contract.address.clone().to_string(), token_id.clone(), msg.clone()).unwrap();

        // stake nft
        let timestamp = env.block.time.seconds();
        stake_function(deps.as_mut(), info.clone(), env.clone(), timestamp, cw721_contract_address.clone(), config.clone(), staker.clone(), token_id.clone());

        let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());
        let max_period = 1000;

        // in config, 1 period is 180 sec
        let now = env.block.time.seconds() + 180;
        let start_timestamp = check_start_timestamp(deps.as_mut()).unwrap();

        // compute rewards
        compute_rewards_function(deps.as_mut(), staker_tokenid_key.clone(), max_period.clone(), now.clone(), start_timestamp.clone(), config.clone());

        // comput rewards after 200 seconds
        let now = now + 200;
        compute_rewards_function(deps.as_mut(), staker_tokenid_key.clone(), max_period.clone(), now.clone(), start_timestamp.clone(), config.clone());

        // unstake
        let timestamp = now + 1;
        unstake_function(deps.as_mut(), info.clone(), env.clone(), cw721_contract_address.clone(), config.clone(), staker.clone(), token_id.clone(), timestamp);
        cw721_contract.transfer_nft(deps.as_mut(), env.clone(), contract_info.clone(), staker.clone(), token_id.clone()).unwrap();

        // re stake
        let timestamp = timestamp + 120;
        cw721_contract.send_nft(deps.as_mut(), env.clone(), staker_info.clone(), env.contract.address.clone().to_string(), token_id.clone(), msg.clone()).unwrap();
        stake_function(deps.as_mut(), info.clone(), env.clone(), timestamp, cw721_contract_address.clone(), config.clone(), staker.clone(), token_id.clone());

        // compute rewards after 180 seconds from re-stake
        let now = timestamp + 180;
        compute_rewards_function(deps.as_mut(), staker_tokenid_key.clone(), max_period.clone(), now.clone(), start_timestamp.clone(), config.clone());
    }

    fn compute_rewards_function(
        mut deps: DepsMut,
        staker_tokenid_key: String,
        max_period: u64,
        now: u64,
        start_timestamp: u64,
        config: Config,
    ) -> (Claim, NextClaim) {

        let mut claim = Claim {
            start_period: 0,
            periods: 0,
            amount: 0,
        };
    
        assert_ne!(max_period, 0);

        let mut next_claim = NEXT_CLAIMS.may_load(deps.branch().storage, staker_tokenid_key.clone()).unwrap().unwrap();
        claim.start_period = next_claim.period;
        assert_ne!(claim.start_period, 0);
    
        let end_claim_period = get_current_period(now, start_timestamp, config.clone()).unwrap();
        assert_ne!(next_claim.period, end_claim_period);

        let staker_history = STAKER_HISTORIES.may_load(deps.storage, staker_tokenid_key.clone()).unwrap().unwrap();
        assert!(staker_history[0].is_staked);
    
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
    
        claim.periods = end_claim_period - next_claim.period;
        if max_period < claim.periods {
            claim.periods = max_period;
        }
    
        let end_claim_period = next_claim.period + claim.periods;
    
        while next_claim.period != end_claim_period {
            let next_period_start_cycle = next_claim.period * config.clone().period_length_in_cycles + 1;
            let reward_per_cycle = REWARDS_SCHEDULE.may_load(deps.storage).unwrap();
            assert!(!reward_per_cycle.is_none());

            let reward_per_cycle = reward_per_cycle.unwrap();
    
            let mut start_cycle = next_period_start_cycle - config.clone().period_length_in_cycles;
            let mut end_cycle = 0;
    
            while end_cycle != next_period_start_cycle {
                if staker_snapshot.start_cycle > start_cycle {
                    start_cycle = staker_snapshot.start_cycle;
                }
    
                end_cycle = next_period_start_cycle;
                if staker_snapshot.is_staked && reward_per_cycle != 0 {
                    let mut snapshot_reward = (end_cycle - start_cycle) as u128 * reward_per_cycle;
                    snapshot_reward = snapshot_reward;
                    claim.amount = claim.amount.add(snapshot_reward)
                }
    
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

        (claim, next_claim)

    }

    fn claim_rewards_function(
        mut deps: DepsMut,
        info: MessageInfo,
        _env: Env,
        max_period: u64,
        token_id: String,
        config: Config,
        timestamp: u64,
    ){
        let start_timestamp = check_start_timestamp(deps.branch()).unwrap();
        check_disable(deps.branch()).unwrap();

        let staker = info.clone().sender.to_string();
        let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());

        let next_claim = NEXT_CLAIMS.may_load(deps.storage, staker_tokenid_key.clone()).unwrap();
        assert!(!next_claim.is_none());

        let next_claim = next_claim.unwrap();
        let now = timestamp;

        let compute_rewards = compute_rewards_function(deps.branch(), staker_tokenid_key.clone(), max_period, now, start_timestamp, config.clone());

        let claim = compute_rewards.0;
        let new_next_claim = compute_rewards.1;

        let staker_history = STAKER_HISTORIES.may_load(deps.storage, staker_tokenid_key.clone()).unwrap();
        assert!(!staker_history.is_none());

        let mut staker_history = staker_history.unwrap();
        while next_claim.staker_snapshot_index < new_next_claim.staker_snapshot_index {
            let delete_index = next_claim.staker_snapshot_index + 1;
            staker_history.remove(delete_index as usize);
            STAKER_HISTORIES.save(deps.storage, staker_tokenid_key.clone(), &staker_history).unwrap();
        }

        assert_ne!(claim.periods, 0);
        assert_ne!(next_claim.period, 0);

        let last_staker_snapshot = staker_history[(staker_history.len() - 1) as usize];
        let last_claimed_cycle = (claim.start_period + claim.periods - 1) * config.period_length_in_cycles;
        if last_claimed_cycle >= last_staker_snapshot.start_cycle && last_staker_snapshot.is_staked == false {
            NEXT_CLAIMS.remove(deps.storage, staker_tokenid_key.clone());
        } else {
            NEXT_CLAIMS.save(deps.storage, staker_tokenid_key.clone(), &new_next_claim).unwrap();
        }

        assert_ne!(claim.amount, 0);

        execute_token_contract_transfer(config.rewards_token_contract, staker, claim.amount).unwrap();
        
    }
}