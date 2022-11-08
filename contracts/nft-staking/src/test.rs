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
    use crate::execute::{add_rewards_pool, add_rewards_for_periods, start};
    use crate::handler::{get_cycle, update_histories, IS_STAKED, get_period, check_start_timestamp, check_disable, staker_tokenid_key, get_current_period, execute_token_contract_transfer};
    use crate::state::{Config, CONFIG_STATE, TOTAL_REWARDS_POOL, REWARDS_SCHEDULE, DISABLE, NEXT_CLAIMS, NextClaim, TOKEN_INFOS, TokenInfo, Claim, STAKER_HISTORIES, Snapshot, MAX_COMPUTE_PERIOD};
    use crate::error::ContractError;

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
        MAX_COMPUTE_PERIOD.save(deps.storage, &2500).unwrap();
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
            let new_next_claim = NextClaim::new(current_period, 0);

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

        let new_token_info = TokenInfo::stake(staker.clone(), IS_STAKED, current_cycle);
        
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

            let token_info = TokenInfo::unstake(!is_staked, token_infos.clone().unwrap().deposit_cycle, current_cycle);

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
        let periods = 1000;

        // in config, 1 period is 180 sec
        let now = env.block.time.seconds() + 180;
        let start_timestamp = check_start_timestamp(deps.as_mut()).unwrap();

        // compute rewards
        compute_rewards_function(deps.as_mut(), staker_tokenid_key.clone(), periods.clone(), now.clone(), start_timestamp.clone(), config.clone());

        // comput rewards after 200 seconds
        let now = now + 200;
        compute_rewards_function(deps.as_mut(), staker_tokenid_key.clone(), periods.clone(), now.clone(), start_timestamp.clone(), config.clone());

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
        compute_rewards_function(deps.as_mut(), staker_tokenid_key.clone(), periods.clone(), now.clone(), start_timestamp.clone(), config.clone());
    }

    fn compute_rewards_function(
        mut deps: DepsMut,
        staker_tokenid_key: String,
        periods: u64,
        now: u64,
        start_timestamp: u64,
        config: Config,
    ) -> (Claim, NextClaim) {

        let max_compute_period = MAX_COMPUTE_PERIOD.load(deps.storage).unwrap();
        assert!(periods < max_compute_period);

        let mut claim = Claim::default();
    
        assert_ne!(periods, 0);

        let mut next_claim = NEXT_CLAIMS.may_load(deps.branch().storage, staker_tokenid_key.clone()).unwrap().unwrap();
        claim.start_period = next_claim.period;
        assert_ne!(claim.start_period, 0);
    
        let end_claim_period = get_current_period(now, start_timestamp, config.clone()).unwrap();
        assert_ne!(next_claim.period, end_claim_period);

        let staker_history = STAKER_HISTORIES.may_load(deps.storage, staker_tokenid_key.clone()).unwrap().unwrap();
        assert!(staker_history[0].is_staked);
    
        let s_state_data = staker_history[next_claim.clone().staker_snapshot_index as usize].clone();
        let mut staker_snapshot = Snapshot::new(s_state_data.is_staked, s_state_data.start_cycle);
    
        let mut next_staker_snapshot = Snapshot::default();
    
        if next_claim.staker_snapshot_index != staker_history.clone().len() as u64 - 1 {
            let s_data = &staker_history.clone()[(next_claim.staker_snapshot_index + 1) as usize];
            next_staker_snapshot = Snapshot::new(s_data.is_staked, s_data.start_cycle);
        }
    
        claim.periods = end_claim_period - next_claim.period;
        if periods < claim.periods {
            claim.periods = periods;
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
        periods: u64,
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

        let compute_rewards = compute_rewards_function(deps.branch(), staker_tokenid_key.clone(), periods, now, start_timestamp, config.clone());

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