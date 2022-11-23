#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, CosmosMsg, StdError};
use cw2::{set_contract_version, get_contract_version};
use cw20::{Cw20ReceiveMsg, Expiration};
use cw721::Cw721ReceiveMsg;

use crate::error::{ContractError};
use crate::handler::{execute_token_contract_transfer, get_cycle, get_period, update_histories, IS_STAKED, check_start_timestamp, check_disable, check_contract_owner, execute_transfer_nft_unstake, compute_rewards, staker_tokenid_key, query_rewards_token_balance, is_valid_cycle_length, is_valid_period_length, manage_number_nfts, contract_info, check_contract_owner_only, check_unbonding_end, check_rewards_pool_balance, CHECK_REWARDS_POOL_AIM_EMPTY, CHECK_REWARDS_POOL_AIM_BOTH, CHECK_REWARDS_POOL_AIM_INSUFFICIENT};
use crate::msg::{ExecuteMsg, InstantiateMsg, SetConfigMsg, MigrateMsg};
use crate::state::{Config, CONFIG_STATE, START_TIMESTAMP, REWARDS_SCHEDULE, TOTAL_REWARDS_POOL, DISABLE, NEXT_CLAIMS, NextClaim, TOKEN_INFOS, TokenInfo, STAKER_HISTORIES, Claim, NUMBER_OF_STAKED_NFTS, MAX_COMPUTE_PERIOD, GRANTS, Grant, UNBONDING_DURATION, UNBONDING, BONDED};

// version info for migration info
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
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

    // default max compute period = 2500.
    // default unbonding duration = 1814400 (= 3 weeks).
    let default_max_compute_period: u64 = 2_500;
    let default_unbonding_duration: u64 = 1_814_400;

    // Default of total rewards pool is zero and of disable state is false.
    TOTAL_REWARDS_POOL.save(deps.storage, &0)?;
    DISABLE.save(deps.storage, &false)?;
    NUMBER_OF_STAKED_NFTS.save(deps.storage, &0)?;
    MAX_COMPUTE_PERIOD.save(deps.storage, &default_max_compute_period)?;
    UNBONDING_DURATION.save(deps.storage, &default_unbonding_duration)?;

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
        ExecuteMsg::SetConfig(msg) => set_config(deps, info, env, config, msg),
        ExecuteMsg::Grant { address, expires } => grant(deps, info, config, address, expires),
        ExecuteMsg::Revoke { address } => revoke(deps, info, config, address),
        ExecuteMsg::AddRewardsForPeriods { rewards_per_cycle } => add_rewards_for_periods(deps, env, info, rewards_per_cycle, config),
        ExecuteMsg::Receive (msg) => add_rewards_pool(deps, info, env, config, msg),
        ExecuteMsg::SetMaxComputePeriod { new_max_compute_period } => set_max_compute_period(deps, info, env, new_max_compute_period, config),
        ExecuteMsg::SetUnbondingDuration { new_unbonding_duration } => set_unbonding_duration(deps, info, env, config, new_unbonding_duration),
        ExecuteMsg::Start {} => start(deps, info, env, config),
        ExecuteMsg::Disable {} => disable(deps, info, env, config),
        ExecuteMsg::Enable {} => enable(deps, info, env, config),
        ExecuteMsg::WithdrawRewardsPool { amount } => withdraw_rewards_pool(deps, info, env, config, amount),
        ExecuteMsg::WithdrawAllRewardsPool {} => withdraw_all_rewards_pool(deps, info, env, config),
        ExecuteMsg::ReceiveNft(msg) => stake_nft(deps, env, info, config, msg),
        ExecuteMsg::UnstakeNft { token_id, claim_recipient_address } => unstake_nft(deps, env, info, config, token_id, claim_recipient_address),
        ExecuteMsg::ClaimRewards { periods, token_id, claim_recipient_address } => claim_rewards(deps, info, env, periods, token_id, config, claim_recipient_address),
    }
}

// change configuration.
pub fn set_config(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
    msg: SetConfigMsg,
) -> Result<Response, ContractError> {
    check_contract_owner(deps.branch(), info.clone(), env.clone(), config.clone())?;

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

// grant other account which it will be given a role of contract owner.
pub fn grant(
    deps: DepsMut,
    info: MessageInfo,
    config: Config,
    address: String,
    expires: Option<Expiration>
) -> Result<Response, ContractError> {
    check_contract_owner_only(info.clone(), config.clone())?;

    let grants = GRANTS.may_load(deps.storage, address.clone())?;
    if grants.is_none() {
        let grants_data = Grant::new(address.clone(), expires);
        GRANTS.save(deps.storage, address.clone(), &grants_data)?;
    } else {
        return Err(ContractError::AlreadyGranted { address: address.clone() })
    }

    Ok(Response::new()
        .add_attribute("method", "grant")
        .add_attribute("grant_address", address)
    )
}

// revoke granted address.
pub fn revoke(
    deps: DepsMut,
    info: MessageInfo,
    config: Config,
    address: String,
) -> Result<Response, ContractError> {
    check_contract_owner_only(info, config)?;

    let grants = GRANTS.may_load(deps.storage, address.clone())?;
    if grants.is_none() {
        return Err(ContractError::InvalidGrantedAddress { address: address.clone() })
    } else {
        GRANTS.remove(deps.storage, address.clone())
    }

    Ok(Response::new()
        .add_attribute("method", "revoke")
        .add_attribute("revoke_address", address)
    )
}

// set rewards schedule.
// rewards per cycle can changed by executing add_rewards_for_periods even after start.
// if rewards per cycle are replaced to new value of rewards per cycle, 
// computing rewards are changed immediatly when staker claims rewards. 
pub fn add_rewards_for_periods(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    rewards_per_cycle: u128,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(deps.branch(), info.clone(), env.clone(), config.clone())?;

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
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
    msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    if info.sender.to_string() != config.clone().rewards_token_contract {
        return Err(ContractError::InvalidRewardsTokenContract { 
            rewards_token_contract: config.clone().rewards_token_contract, 
            requester: info.sender.to_string(), 
        })
    }

    check_contract_owner(deps.branch(), contract_info(msg.clone()).unwrap(), env.clone(), config.clone())?;

    let total_rewards_pool = TOTAL_REWARDS_POOL.load(deps.storage)?;
    let rewards = total_rewards_pool + msg.amount.clone().u128();

    TOTAL_REWARDS_POOL.save(deps.storage, &rewards)?;

    Ok(Response::new()
        .add_attribute("method", "add_rewards_pool")
        .add_attribute("added_rewards", msg.amount.to_string())
        .add_attribute("total_rewards", rewards.to_string())
        .add_attribute("send_from", info.sender)
    )
}

// change max_compute_period that default value is 2500.
// nft staking contract needs max_compute_period to avoid restriction about query gas limit of wasmd(defaultSmartQueryGasLimit is 3,000,000).  
pub fn set_max_compute_period (
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    new_max_compute_period: u64,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(deps.branch(), info.clone(), env.clone(), config.clone())?;
    if new_max_compute_period <= 0 {
        return Err(ContractError::InvalidSetMaxPeriod {})
    }

    let previous_max_compute_period = MAX_COMPUTE_PERIOD.load(deps.storage)?;
    MAX_COMPUTE_PERIOD.save(deps.storage, &new_max_compute_period)?;

    Ok(Response::new()
        .add_attribute("method", "set_max_compute_period")
        .add_attribute("previous_max_compute_period", previous_max_compute_period.to_string())
        .add_attribute("new_max_compute_period", new_max_compute_period.to_string())
    )
}

// change unbonding_duration that default value is 1814400.
// when a staker requests to unstake nft token id, the owner of token id is changed to the staker from nft staking contract after unbonding duration.
// the staker is not able to unstake the nft token id, but also cannot claim rewards when the bond status is "UNBONDING".
pub fn set_unbonding_duration(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
    new_unbonding_duration: u64,
) -> Result<Response, ContractError> {
    check_contract_owner(deps.branch(), info, env, config)?;

    UNBONDING_DURATION.save(deps.storage, &new_unbonding_duration.clone())?;

    Ok(Response::new()
        .add_attribute("method", "set_unbonding_duration")
        .add_attribute("new_unbonding_duration", new_unbonding_duration.to_string())
    )
}

// nft staking contract start.
// every calculating period and cycle are affected by start timestamp.
pub fn start(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(deps.branch(), info.clone(), env.clone(), config.clone())?;

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
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(deps.branch(), info.clone(), env.clone(), config.clone())?;

    DISABLE.save(deps.storage, &true)?;

    Ok(Response::new()
        .add_attribute("method", "disable")
        .add_attribute("disable", true.to_string())
    )
}

// if the nft staking contract is disabled and the contract owner want to activate again, 
// execute enable function.
pub fn enable(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(deps.branch(), info.clone(), env.clone(), config.clone())?;

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
// the nft staking contract's balances of token rewards which is value of requested amount are transferred to contract owner.
pub fn withdraw_rewards_pool(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
    amount: u128,
) -> Result<Response, ContractError> {
    check_contract_owner(deps.branch(), info.clone(), env.clone(), config.clone())?;

    let disabled = check_disable(deps.branch())?;
    let rewards_token_contract = config.clone().rewards_token_contract;
    let owner = info.clone().sender;

    check_rewards_pool_balance(deps.branch(), env.clone(), config.clone(), CHECK_REWARDS_POOL_AIM_INSUFFICIENT, Some(amount.clone()))?;
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
// the nft staking contract's all balances are transferred to contract owner.
pub fn withdraw_all_rewards_pool(
    mut deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
) -> Result<Response, ContractError> {
    check_contract_owner(deps.branch(), info.clone(), env.clone(), config.clone())?;

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
    // check empty total supply rewards pool.
    let total_rewards_pool = TOTAL_REWARDS_POOL.may_load(deps.branch().storage)?;
    if total_rewards_pool.is_none() {
        return Err(ContractError::EmptyRewardsPool {})
    }

    // check empty rewards pool of nft staking contract.
    check_rewards_pool_balance(deps.branch(), env.clone(), config.clone(), CHECK_REWARDS_POOL_AIM_EMPTY, None)?;

    // check rewards schedule.
    let rewards_schedule = REWARDS_SCHEDULE.may_load(deps.branch().storage)?;
    if rewards_schedule.is_none() {
        return Err(ContractError::NoneRewardsSchedule {})
    }

    // check the nft must be sended from whitelisted nft contract.
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

    // the nft for staking is managed by mapping staker's address and nft token ID.
    // the staker stakes multi nft and can claim rewards for each nft.
    let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());

    let update_histories_response = update_histories(deps.branch(), staker_tokenid_key.clone(), IS_STAKED, current_cycle)?;

    let token_infos = TOKEN_INFOS.may_load(deps.branch().storage, token_id.clone())?;
    if !token_infos.is_none() {

        // prevent duplication.
        if token_infos.clone().unwrap().is_staked {
            return Err(ContractError::AlreadyStaked {})
        }
        let withdraw_cycle = token_infos.unwrap().withdraw_cycle;

        // cannot re-stake when current cycle of block time is same setup withdraw cycle
        if current_cycle == withdraw_cycle {
            return Err(ContractError::UnstakedTokenCooldown {})
        }    
    }

    let next_claims = NEXT_CLAIMS.may_load(deps.branch().storage, staker_tokenid_key.clone())?;

    // initialise the next claim if it was the first stake for this staker or if 
    // the next claim was re-initialised.
    // i.e. rewards were claimed until the last staker snapshot and the last staker snapshot is not staked.
    if next_claims.is_none() {
        let current_period = get_period(current_cycle, config.clone())?;
        let new_next_claim = NextClaim::new(current_period, 0);

        NEXT_CLAIMS.save(deps.branch().storage, staker_tokenid_key.clone(), &new_next_claim)?;
    }

    let new_token_info = TokenInfo::stake(staker.clone(), IS_STAKED, current_cycle);
    
    TOKEN_INFOS.save(deps.branch().storage, token_id.clone(), &new_token_info)?;
    manage_number_nfts(deps.branch(), true);

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
    claim_recipient_address: Option<String>,
) -> Result<Response, ContractError> {
    let staker = info.clone().sender.to_string();
    let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());
    let token_info = TokenInfo::check_staker(deps.branch(), info.clone(), token_id.clone())?;

    let start_timestamp = check_start_timestamp(deps.branch())?;
    let timestamp = env.block.time.seconds();
    let disable = check_disable(deps.branch())?;
    let is_staked = token_info.clone().is_staked;
    let mut messages: Vec<CosmosMsg> = vec![];

    // the bond status of requested nft that is "BONDED" is replaced to "UNBONDING".
    if token_info.bond_status == BONDED {
        let token_info_unbonding = TokenInfo::unstake_unbonding(
            staker.clone(), 
            is_staked, 
            token_info.clone().deposit_cycle, 
            token_info.clone().withdraw_cycle,
            timestamp.clone(),
        );
        TOKEN_INFOS.save(deps.branch().storage, token_id.clone(), &token_info_unbonding)?;

        return Ok(Response::new()
            .add_attribute("method", "unstake_nft")
            .add_attribute("request_unstake_time", timestamp.to_string())
            .add_attribute("bond_status", UNBONDING)
        )
    }

    // the nft actually is unstaked that nft owner is changed to the staker, 
    // if the bond status of the nft is "UNBONDING" and current timestamp is bigger than 
    // sum of requsted unstake time and unbonding duration that is already set up.
    check_unbonding_end(deps.as_ref(), token_info.clone(), timestamp.clone())?; 

    let current_cycle = get_cycle(timestamp, start_timestamp, config.clone())?;

    // before unstake the nft by staker, rewards token balances are transfer to staker.
    let max_compute_period = MAX_COMPUTE_PERIOD.load(deps.branch().storage)?;
    let mut remain_rewards = true;
    let mut remain_rewards_value: u128 = 0;
    let mut recipient: Option<String> = Some(staker.clone());
    if !claim_recipient_address.is_none() {
        recipient = claim_recipient_address;
    }

    if !disable {
        // ensure that at least an entire cycle has elapsed before unstaking the token to avoid
        // an exploit where a full cycle would be claimable if staking just before the end
        // of a cycle and unstaking right after start of the new cycle.
        if !(current_cycle - token_info.clone().deposit_cycle >= 2) {
            return Err(ContractError::TokenSteelFrozen {})
        }

        let token_info_unbonded = TokenInfo::unstake_unbonded(
            staker.clone(), 
            is_staked, 
            token_info.clone().deposit_cycle, 
            token_info.clone().withdraw_cycle,
            token_info.clone().req_unbond_time,
        );
        TOKEN_INFOS.save(deps.branch().storage, token_id.clone(), &token_info_unbonded)?;

        while remain_rewards {
            let compute_reward = compute_rewards(
                deps.as_ref(), 
                staker_tokenid_key.clone(), 
                max_compute_period,
                timestamp,
                start_timestamp,
                config.clone(),
                token_id.clone()
            )?;

            if compute_reward.0.amount != 0 {
                remain_rewards_value = remain_rewards_value + compute_reward.0.amount;
                // next claim set last computed rewards.
                NEXT_CLAIMS.save(deps.branch().storage, staker_tokenid_key.clone(), &compute_reward.1)?;
            } else {
                remain_rewards = false
            }
        }
        update_histories(deps.branch(), staker_tokenid_key.clone(), !is_staked, current_cycle)?;

        // clear the token owner to ensure it cannot be unstaked again without being re-staked.
        // set the withdrawal cycle to ensure it cannot be re-staked during the same cycle.
        let token_info = TokenInfo::unstake(!is_staked, token_info.clone().deposit_cycle, current_cycle);

        TOKEN_INFOS.save(deps.branch().storage, token_id.clone(), &token_info)?;
    }

    if remain_rewards_value != 0 {
        // check empty and sufficient rewards pool of nft staking contract.
        // for checking sufficient rewards pool, must input amount.
        check_rewards_pool_balance(deps.branch(), env.clone(), config.clone(), CHECK_REWARDS_POOL_AIM_BOTH, Some(remain_rewards_value.clone()))?;
        let claim_message = execute_token_contract_transfer(config.clone().rewards_token_contract, recipient.clone().unwrap(), remain_rewards_value.clone())?;
        let claim_cosmos_msg = claim_message
            .get(0)
            .unwrap()
            .clone();

        messages.push(claim_cosmos_msg)
    }
    
    // next claims of specified nft are eliminated.
    NEXT_CLAIMS.remove(deps.branch().storage, staker_tokenid_key.clone());
    manage_number_nfts(deps.branch(), false);

    messages.push(execute_transfer_nft_unstake(token_id, staker, config.white_listed_nft_contract)?);

    Ok(Response::new()
        .add_attribute("method", "unstake_nft")
        .add_attribute("request_unstake_time", timestamp.to_string())
        .add_attribute("claim_remain_rewards", remain_rewards_value.to_string())
        .add_attribute("recipient_remain_rewards", recipient.unwrap())
        .add_messages(messages)
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
    periods: u64,
    token_id: String,
    config: Config,
    claim_recipient_address: Option<String>,
) -> Result<Response, ContractError> {
    let start_timestamp = check_start_timestamp(deps.branch())?;
    check_disable(deps.branch())?;

    let staker = info.clone().sender.to_string();
    let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());

    let check_token_info = TOKEN_INFOS.may_load(deps.branch().storage, token_id.clone())?;
    if check_token_info.is_none() {
        return Err(ContractError::InvalidTokenId {})
    }

    let token_info = check_token_info.unwrap();

    // although the time reaches unbonded status, the staker should not claim directly.
    // the staker is able to get balances of rewards only execute unstake function.
    if token_info.bond_status == UNBONDING {
        return Err(ContractError::TokenIdIsUnbonding {})
    }

    let next_claim = NEXT_CLAIMS.may_load(deps.storage, staker_tokenid_key.clone())?;
    if next_claim.is_none() {
        return Err(ContractError::EmptyNextClaim {})
    }
    let next_claim = next_claim.unwrap();

    let now = env.block.time.seconds();
    let claim: Claim;
    let new_next_claim: NextClaim;
    let compute_rewards = compute_rewards(deps.as_ref(), staker_tokenid_key.clone(), periods, now, start_timestamp, config.clone(), token_id.clone());
    match compute_rewards {
        Ok(t) => {
            claim = t.0;
            new_next_claim = t.1;
        },
        Err(e) => {
            return Err(e)
        }
    }

    // check sufficient rewards pool of nft staking contract.
    // for checking sufficient rewards pool, must input amount.
    check_rewards_pool_balance(deps.branch(), env.clone(), config.clone(), CHECK_REWARDS_POOL_AIM_INSUFFICIENT, Some(claim.amount.clone()))?;

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

    let mut exist_next_claim = true;
    let last_staker_snapshot = staker_history[(staker_history.len() - 1) as usize];
    let last_claimed_cycle = (claim.start_period + claim.periods - 1) * config.period_length_in_cycles;

    // the claim reached the last staker snapshot and nothing is staked in the last staker snapshot.
    if last_claimed_cycle >= last_staker_snapshot.start_cycle && last_staker_snapshot.is_staked == false {
        
        // re-init the next claim.
        NEXT_CLAIMS.remove(deps.storage, staker_tokenid_key.clone());
        exist_next_claim = false;
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
    let message = execute_token_contract_transfer(config.rewards_token_contract, recipient.clone(), claim.amount)?;

    Ok(Response::new()
        .add_attribute("method", "claim_rewards")
        .add_attribute("claim_start_period", claim.start_period.to_string())
        .add_attribute("claim_periods", claim.periods.to_string())
        .add_attribute("claim_amount", claim.amount.to_string())
        .add_attribute("claim_recipient", recipient.to_string())
        .add_attribute("exist_next_claim", exist_next_claim.to_string())
        .add_messages(message)
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(
    deps: DepsMut, 
    _env: Env, 
    _msg: MigrateMsg
) -> Result<Response, ContractError> {
    let ver = get_contract_version(deps.storage)?;
    if ver.contract != CONTRACT_NAME {
        return Err(StdError::generic_err("Can only upgrade from same type").into());
    }

    #[allow(clippy::cmp_owned)]
    if ver.version >= CONTRACT_VERSION.to_string() {
        return Err(StdError::generic_err("Cannot upgrade from a newer version").into());
    }

    // set the new version
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}