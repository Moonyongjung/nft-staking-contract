#[cfg(test)]
mod tests{
    use std::ops::Add;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MOCK_CONTRACT_ADDR};
    use cosmwasm_std:: {MessageInfo, DepsMut, Env, Empty, MemoryStorage, OwnedDeps, Addr, Uint128, BlockInfo, Timestamp, TransactionInfo, ContractInfo, to_binary, Response, Binary, CosmosMsg, WasmMsg};
    use cw20::{Cw20Coin, MinterResponse, Cw20ReceiveMsg, Cw20ExecuteMsg, BalanceResponse, Expiration};
    use cw20_base::contract::{instantiate, execute, query_balance};
    use cw721::{Cw721ReceiveMsg, Cw721Execute};
    use cw721_base::{Cw721Contract, Extension, InstantiateMsg as Cw721BaseInstantiateMsg, ExecuteMsg as Cw721BaseExecuteMsg, MintMsg};
    use cw20_base::{msg::InstantiateMsg as Cw20InstantiateMsg};
    use crate::execute::{instantiate as nft_staking_instantiate, add_rewards_pool, add_rewards_for_periods, start, grant, set_config, revoke, disable, claim_rewards, unstake_nft, withdraw_all_rewards_pool};
    use crate::handler::{get_cycle, update_histories, IS_STAKED, get_period, check_start_timestamp, check_disable, staker_tokenid_key, get_current_period, manage_number_nfts, check_unbonding_end, compute_rewards};
    use crate::msg::{InstantiateMsg, SetConfigMsg};
    use crate::state::{Config, CONFIG_STATE, TOTAL_REWARDS_POOL, REWARDS_SCHEDULE, NEXT_CLAIMS, NextClaim, TOKEN_INFOS, TokenInfo, STAKER_HISTORIES, MAX_COMPUTE_PERIOD, UNBONDING_DURATION, BONDED, UNBONDING, START_TIMESTAMP, Claim};
    use crate::error::ContractError;

    const CONTRACT_NAME: &str = "CW721CTRT";
    const SYMBOL: &str = "CW721";
    const MINTER: &str = "xpla1j55tymfdys9n7k0dq6xmyd4hgfelp9jghzympt";
    const GRANTER: &str = "xpla1xfdddeclg0r25l0fg5hpm5qfvdqqwz8pumvg0k";
    const STAKER: &str = "xpla1ma4peq833n2k3t7u2f60w420ltx8nvz0g0vwlu";
    const TOKEN_ID: &str = "token_id_test_0";
    const ADD_REWARDS_POOL: u128 = 2000000000;
    const CYCLE_LENGTH_IN_SECONDS: u64 = 60;
    const PERIOD_LENGTH_IN_CYCLES: u64 = 3;
    const REWARDS_PER_CYCLE: u128 = 17;
    const DEFAULT_MAX_COMPUTE_PERIOD: u64 = 2500;

    #[test]
    fn test_set_config() {
        // test environment
        let (mut deps, info, env, _cw721_contract, _cw721_contract_address, config, _staker, _token_id) = test_environment();

        let set_config_msg = SetConfigMsg {
            cycle_length_in_seconds: Some(100),
            period_length_in_cycles: None,
            white_listed_nft_contract: Some("other_cw721_contract".to_string()),
            rewards_token_contract: None,
        };

        // set config test
        set_config(deps.as_mut(), info, env, config, set_config_msg).unwrap();

        let config = CONFIG_STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(config.cycle_length_in_seconds, 100);
        assert_eq!(config.period_length_in_cycles, PERIOD_LENGTH_IN_CYCLES);
        assert_eq!(config.white_listed_nft_contract, "other_cw721_contract");
        assert_eq!(config.rewards_token_contract, mock_env_cw20().contract.address);
    }

    #[test]
    fn test_grant_and_revoke() {
        // test environment
        let (mut deps, info, env, _cw721_contract, _cw721_contract_address, config, _staker, _token_id) = test_environment();

        let address = GRANTER.to_string();
        let expiration = Expiration::default();

        // grant
        grant(deps.as_mut(), info.clone(), config.clone(), address.clone(), Some(expiration)).unwrap();

        let granter_info = mock_info(address.as_str(), &[]);
        let set_config_msg = SetConfigMsg {
            cycle_length_in_seconds: Some(100),
            period_length_in_cycles: None,
            white_listed_nft_contract: Some("other_cw721_contract".to_string()),
            rewards_token_contract: None,
        };

        // check that granter can execute set_config
        set_config(deps.as_mut(), granter_info.clone(), env.clone(), config.clone(), set_config_msg.clone()).unwrap();

        let config = CONFIG_STATE.load(deps.as_mut().storage).unwrap();
        assert_eq!(config.cycle_length_in_seconds, 100);
        assert_eq!(config.period_length_in_cycles, PERIOD_LENGTH_IN_CYCLES);
        assert_eq!(config.white_listed_nft_contract, "other_cw721_contract");
        assert_eq!(config.rewards_token_contract, mock_env_cw20().contract.address);

        // revoke
        revoke(deps.as_mut(), info, config.clone(), address).unwrap();

        // revoked granter cannot execute set_config
        let result = set_config(deps.as_mut(), granter_info.clone(), env.clone(), config.clone(), set_config_msg.clone());        
        assert_eq!(ContractError::Unauthorized {}.to_string(), result.err().unwrap().to_string());
    }

    #[test]
    fn test_add_rewards_for_period() {
        // test environment
        let (mut deps, info, env, _cw721_contract, _cw721_contract_address, config, _staker, _token_id) = test_environment();

        let rewards_per_cycle = REWARDS_PER_CYCLE;
        add_rewards_for_periods(deps.as_mut(), env.clone(), info.clone(), rewards_per_cycle.clone(), config.clone()).unwrap();

        // normal case
        let rewards_schedule = REWARDS_SCHEDULE.load(deps.as_mut().storage).unwrap();
        assert_eq!(REWARDS_PER_CYCLE, rewards_schedule);

        // error case that rewards per cycle is zero
        let rewards_per_cycle: u128 = 0;
        let result = add_rewards_for_periods(deps.as_mut(), env.clone(), info.clone(), rewards_per_cycle.clone(), config.clone());
        assert_eq!(ContractError::InvalidRewardsSchedule {}.to_string(), result.err().unwrap().to_string())
    }

    #[test]
    fn test_disable() {
        // set environment and do stake
        let (mut deps, info, env, _cw721_contract, _cw721_contract_address, config, staker, token_id) = do_stake();

        // contract disabled
        disable(deps.as_mut(), info.clone(), env.clone(), config.clone()).unwrap();

        let periods: u64 = 10;
        let claim_recipient_address = None;

        // cannot run functions
        let res = claim_rewards(deps.as_mut(), info.clone(), env.clone(), periods, token_id.clone(), config.clone(), claim_recipient_address.clone());
        assert_eq!(ContractError::Disabled {}.to_string(), res.err().unwrap().to_string());

        let staker_info = mock_info(staker.as_str(), &[]);
        let res = unstake_nft(deps.as_mut(), env.clone(), staker_info.clone(), config.clone(), token_id.clone(), claim_recipient_address.clone());
        assert_eq!(ContractError::Disabled {}.to_string(), res.err().unwrap().to_string());

        let res = withdraw_all_rewards_pool(deps.as_mut(), info.clone(), env.clone(), config.clone());
        assert_eq!(ContractError::Disabled {}.to_string(), res.err().unwrap().to_string());
    }

    #[test]
    fn test_stake() {
        do_stake();
    }

    #[test]
    fn test_claim() {
        // do stake
        let (mut deps, _info, env, _cw721_contract, _cw721_contract_address, config, staker, token_id) = do_stake();

        // time passed by 5000 seconds
        let timestamp = env.block.time.seconds() + 5000;
        let staker_info = mock_info(staker.as_str(), &[]);
        let request_claim_period = 5;
        let claim_recipient_address = None;
        let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());

        // claim
        let res = claim_rewards_function(deps.as_mut(), staker_info.clone(), env.clone(), request_claim_period, token_id.clone(), config.clone(), claim_recipient_address.clone(), timestamp.clone());

        // --------------------------------
        // check after run claim function
        let staker_rewards = query_balance(deps.as_ref(), staker.clone()).unwrap();
        let contract_balance = query_balance(deps.as_ref(), env.contract.address.to_string()).unwrap();
        let next_claim = NEXT_CLAIMS.load(deps.as_mut().storage, staker_tokenid_key.clone()).unwrap();
        
        // deposit cycle = 1.
        // cycle length in seconds is 60 and period length in cycles is 3 for test.
        // rewards per cycle is 17.
        // rewards are sufficient because of a lot of time passed after staked.
        // request claim period is 5.

        // the equation of claimable rewards value = 5 * 3 * 17 = 255
        // and next claim is 6 because rewards are claimed until period 5.
        assert_eq!(255, staker_rewards.balance.u128());
        assert_eq!(1999999745, contract_balance.balance.u128());
        assert_eq!(6, next_claim.period);
        assert_eq!(res.as_ref().unwrap().attributes.get(2).unwrap().value, staker);
        assert_eq!(res.as_ref().unwrap().attributes.get(3).unwrap().value, 255.to_string());
    }

    #[test]
    fn test_claim_exceeding_max_compute_period() {
        // do stake
        let (mut deps, _info, env, _cw721_contract, _cw721_contract_address, config, staker, token_id) = do_stake();

        let timestamp = env.block.time.seconds() + 5000;
        let staker_info = mock_info(staker.as_str(), &[]);

        // exceed max compute period that default value is 2500
        let request_claim_period = 3000;
        let claim_recipient_address = None;

        // claim error
        let res = claim_rewards_function(deps.as_mut(), staker_info.clone(), env.clone(), request_claim_period, token_id.clone(), config.clone(), claim_recipient_address.clone(), timestamp.clone());
        assert_eq!(ContractError::InvalidMaxPeriod {
            periods: request_claim_period,
            max_compute_period: DEFAULT_MAX_COMPUTE_PERIOD,
        }.to_string(), res.err().unwrap().to_string());
    }

    #[test]
    fn test_claim_other_recipient_address() {
        // do stake
        let (mut deps, _info, env, _cw721_contract, _cw721_contract_address, config, staker, token_id) = do_stake();

        let timestamp = env.block.time.seconds() + 5000;
        let staker_info = mock_info(staker.as_str(), &[]);
        let request_claim_period = 5;

        // set the recipient address is granter address
        let claim_recipient_address = Some(GRANTER.to_string());

        // claim
        let res = claim_rewards_function(deps.as_mut(), staker_info.clone(), env.clone(), request_claim_period, token_id.clone(), config.clone(), claim_recipient_address.clone(), timestamp.clone());

        // --------------------------------
        // check after run claim function
        let staker_rewards = query_balance(deps.as_ref(), staker.clone()).unwrap();
        let contract_balance = query_balance(deps.as_ref(), env.contract.address.to_string()).unwrap();
        let granter_rewards = query_balance(deps.as_ref(), GRANTER.to_string()).unwrap();

        // the granter receives claim rewards
        assert_eq!(255, granter_rewards.balance.u128());
        assert_eq!(1999999745, contract_balance.balance.u128());
        assert_eq!(0, staker_rewards.balance.u128());
        assert_eq!(res.as_ref().unwrap().attributes.get(2).unwrap().value, GRANTER.to_string());
        assert_eq!(res.as_ref().unwrap().attributes.get(3).unwrap().value, 255.to_string());
    }


    #[test]
    fn test_claim_while_unbonding_duration() {
        // do stake
        let (mut deps, _info, env, _cw721_contract, _cw721_contract_address, config, _staker, token_id) = do_stake();

        let staker_info = mock_info(STAKER, &[]);
        let timestamp = env.block.time.seconds() + 2000;
        let claim_recipient_address = None;
        let request_claim_period = 5;

        // request unbond nft. the nft is unbonding
        test_unstake_function(deps.as_mut(), env.clone(), staker_info.clone(), config.clone(), token_id.clone(), claim_recipient_address.clone(), timestamp.clone()).unwrap();
        let token_info = TOKEN_INFOS.load(deps.as_mut().storage, token_id.clone());
        assert_eq!(token_info.unwrap().bond_status, UNBONDING);

        // claim error
        let res = claim_rewards_function(deps.as_mut(), staker_info.clone(), env.clone(), request_claim_period, token_id.clone(), config.clone(), claim_recipient_address.clone(), timestamp.clone());
        assert_eq!(ContractError::TokenIdIsUnbonding {}.to_string(), res.err().unwrap().to_string());
    }

    #[test]
    fn test_claim_empty_rewards_pool() {
        // do stake
        let (mut deps, info, env, _cw721_contract, _cw721_contract_address, config, _staker, token_id) = do_stake();

        let staker_info = mock_info(STAKER, &[]);
        let timestamp = env.block.time.seconds() + 2000;
        let claim_recipient_address = None;
        let request_claim_period = 5;

        // withdraw all rewards pool
        test_execute_token_contract_transfer(deps.as_mut(), env.clone(), info, MINTER.to_string(), ADD_REWARDS_POOL);

        // claim error
        let res = claim_rewards_function(deps.as_mut(), staker_info.clone(), env.clone(), request_claim_period, token_id.clone(), config.clone(), claim_recipient_address.clone(), timestamp.clone());
        assert_eq!(ContractError::InsufficientRewardsPool {
            rewards_pool_balance: test_query_rewards_token_balance(deps.as_mut(), env.clone().contract.address.to_string()).balance.u128(),
            claim_amount: 255, 
        }.to_string(), res.err().unwrap().to_string());

    }

    #[test]
    fn test_unstake() {
        // do stake
        let (mut deps, _info, env, cw721_contract, _cw721_contract_address, config, staker, token_id) = do_stake();
        
        let staker_info = mock_info(STAKER, &[]);
        let timestamp = env.block.time.seconds() + 2000;

        // not claim to other address
        let claim_recipient_address = None;

        // request unbond nft
        test_unstake_function(deps.as_mut(), env.clone(), staker_info.clone(), config.clone(), token_id.clone(), claim_recipient_address.clone(), timestamp.clone()).unwrap();

        // requested unbonding period value
        let start_timestamp = START_TIMESTAMP.load(deps.as_mut().storage).unwrap();
        let requested_unbonding_period = get_current_period(timestamp, start_timestamp, config.clone()).unwrap();

        let unbonding_duration = UNBONDING_DURATION.load(deps.as_mut().storage).unwrap();
        assert_eq!(unbonding_duration, 1814400);

        // current time is after sum of timestamp and unbonding duration + 1 
        let timestamp = timestamp + unbonding_duration + 1;

        // re-request unstake the nft has "UNBONDED" as bond_status
        test_unstake_function(deps.as_mut(), env.clone(), staker_info.clone(), config.clone(), token_id.clone(), claim_recipient_address.clone(), timestamp.clone()).unwrap();

        // for test, seperate executing transfer nft and unstake function.
        test_execute_transfer_nft_unstake(deps.as_mut(), env.clone(), staker.clone(), token_id, cw721_contract);

        // --------------------------------
        // check after run unstake function
        let staker_rewards = query_balance(deps.as_ref(), staker).unwrap();
        let contract_balance = query_balance(deps.as_ref(), env.contract.address.to_string()).unwrap();

        // deposit cycle = 1.
        // cycle length in seconds is 60 and period length in cycles is 3 for test.
        // requested unbonding time is now + 2000 seconds.
        // rewards per cycle is 17.

        // requested unbonding period that requested unbonding time is included is 12 (last cycle second is 2,160) and previous period that is claimable period is 11(last cycle second is 1,980).
        // claimable rewards is limited at requested unbonding time, although current time is passed by requested unbonding time + unbonding duration(1814400) + 1.
        // so, the equation of claimable rewards value = 1980 / 60 * 17 = 561
        // nft staking contract's rewards pool (i.e. cw20 token balance of contract) is 2000000000 - 561 = 1999999439
        
        assert_eq!(12, requested_unbonding_period);
        assert_eq!(561, staker_rewards.balance.u128());
        assert_eq!(1999999439, contract_balance.balance.u128());
    }

    #[test]
    fn test_unstake_not_reach_unbonding_time() {
        // do stake
        let (mut deps, _info, env, _cw721_contract, _cw721_contract_address, config, _staker, token_id) = do_stake();
                
        let staker_info = mock_info(STAKER, &[]);
        let timestamp = env.block.time.seconds();
        let claim_recipient_address = None;

        // request unbond nft
        test_unstake_function(deps.as_mut(), env.clone(), staker_info.clone(), config.clone(), token_id.clone(), claim_recipient_address.clone(), timestamp.clone()).unwrap();

        // re-request unstake before end of unbonding duration
        let unbonding_duration = UNBONDING_DURATION.load(deps.as_mut().storage).unwrap();
        let before_unbonding_duration = unbonding_duration - 10;
        assert_eq!(1814390, before_unbonding_duration);

        // unbonding duration error
        let res = test_unstake_function(deps.as_mut(), env.clone(), staker_info.clone(), config.clone(), token_id.clone(), claim_recipient_address.clone(), before_unbonding_duration.clone());
        assert_eq!(ContractError::NotReachUnbondingTime {}.to_string(), res.err().unwrap().to_string());
    }

    #[test]
    fn test_unstake_staker_has_alotof_rewards() {
        // do stake
        let (mut deps, _info, env, _cw721_contract, _cw721_contract_address, config, staker, token_id) = do_stake();
        
        let staker_info = mock_info(STAKER, &[]);

        // pass many time enough to exceed max compute period
        let timestamp = env.block.time.seconds() + 10000000;
        let claim_recipient_address = None;

        // current period is much bigger than default max compute period
        let start_timestamp = START_TIMESTAMP.load(deps.as_mut().storage).unwrap();
        let current_period = get_current_period(timestamp, start_timestamp, config.clone()).unwrap();
        assert_eq!(55556, current_period);
        assert!(current_period > DEFAULT_MAX_COMPUTE_PERIOD);

        // request unbond nft
        test_unstake_function(deps.as_mut(), env.clone(), staker_info.clone(), config.clone(), token_id.clone(), claim_recipient_address.clone(), timestamp.clone()).unwrap();

        let unbonding_duration = UNBONDING_DURATION.load(deps.as_mut().storage).unwrap();

        // current time is after sum of timestamp and unbonding duration + 1 
        let timestamp = timestamp + unbonding_duration + 1;

        // re-request unstake the nft has "UNBONDED" as bond_status
        test_unstake_function(deps.as_mut(), env.clone(), staker_info.clone(), config.clone(), token_id.clone(), claim_recipient_address.clone(), timestamp.clone()).unwrap();

        // --------------------------------
        // check after run unstake function
        let staker_rewards = query_balance(deps.as_ref(), staker).unwrap();
        let contract_balance = query_balance(deps.as_ref(), env.contract.address.to_string()).unwrap();

        // deposit cycle = 1.
        // cycle length in seconds is 60 and period length in cycles is 3 for test.
        // requested unbonding period is 55556.
        // rewards per cycle is 17.

        // claimable period is 55555 which is previous period of the requested unbonding period
        // so, the equation of claimable rewards value = 55555 * 3 * 17 = 2,833,305
        // nft staking contract's rewards pool (i.e. cw20 token balance of contract) is 2000000000 - 2833305 = 1997166695
        assert_eq!(2833305, staker_rewards.balance.u128());
        assert_eq!(1997166695, contract_balance.balance.u128());
    }

    // test helpers
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
        let minter = String::from(MINTER);
        let staker = String::from(STAKER);
        let token_id = String::from(TOKEN_ID);

        let add_rewards = Uint128::from(ADD_REWARDS_POOL);
        let send_msg = Binary::from(r#"{add_rewards}"#.as_bytes());

        // nft staking contract
        let mut deps = mock_dependencies();
        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();

        // cw721 contract
        let cw721_contract = setup_contract_cw721(deps.as_mut());
        let cw721_contract_address = mock_env_cw721().contract.address;

        // mint nft token ID
        let mint_msg = Cw721BaseExecuteMsg::Mint(MintMsg::<Extension> {
            token_id: token_id.clone(),
            owner: staker.clone(),
            token_uri: None,
            extension: None,
        });
        cw721_contract
            .execute(deps.as_mut(), mock_env_cw721(), info.clone(), mint_msg)
            .unwrap();

        // cw20 contract
        setup_contract_cw20(deps.as_mut());
        let cw20_contract_address = mock_env_cw20().clone().contract.address;

        // instantiate
        let instantiate_res = do_instantiate(deps.as_mut(), info.clone(), env.clone(), cw721_contract_address.clone().to_string(), cw20_contract_address.clone().to_string());
        assert_eq!(instantiate_res.attributes.get(0).unwrap().value, "instantiate");
        assert_eq!(instantiate_res.attributes.get(1).unwrap().value, minter);
        assert_eq!(instantiate_res.attributes.get(2).unwrap().value, CYCLE_LENGTH_IN_SECONDS.to_string());
        assert_eq!(instantiate_res.attributes.get(3).unwrap().value, PERIOD_LENGTH_IN_CYCLES.to_string());
        assert_eq!(instantiate_res.attributes.get(4).unwrap().value, cw721_contract_address.clone());
        assert_eq!(instantiate_res.attributes.get(5).unwrap().value, cw20_contract_address.clone());

        let config = get_config(deps.as_mut()).unwrap();

        // set reward schedule includes rewards_per_cycle
        let add_rewards_schedule = add_rewards_for_periods(deps.as_mut(), env.clone(), info.clone(), REWARDS_PER_CYCLE, config.clone()).unwrap();
        assert_eq!(add_rewards_schedule.attributes.get(0).unwrap().value, "add_rewards_for_periods");
        assert_eq!(add_rewards_schedule.attributes.get(1).unwrap().value, REWARDS_PER_CYCLE.to_string());

        // minter sends cw20 tokens to contract for supplying rewards pool
        let msg = Cw20ExecuteMsg::Send {
            contract: env.contract.address.clone().to_string(),
            amount: add_rewards.clone(),
            msg: send_msg.clone(),
        };
        let res = execute(deps.as_mut(), mock_env_cw20().clone(), info.clone(), msg.clone()).unwrap();
        assert_eq!(res.messages.len(), 1);

        // received msg
        let msg = Cw20ReceiveMsg {
            sender: minter.clone(),
            amount: add_rewards.clone(),
            msg: send_msg.clone()
        };

        let cm_msg: CosmosMsg<Empty> = CosmosMsg::Wasm(WasmMsg::Execute { 
            contract_addr: MOCK_CONTRACT_ADDR.to_string(), 
            msg: msg.clone().into_binary().unwrap(), 
            funds: vec![]
        });

        assert_eq!(res.messages[0].msg, cm_msg);

        let cw20_info = mock_info(cw20_contract_address.as_str(), &[]);
        let add_rewards_pool = add_rewards_pool(deps.as_mut(), cw20_info.clone(), env.clone(), config.clone(), msg).unwrap();

        // check balance as token rewards pool of nft staking contract
        let balance_response = test_query_rewards_token_balance(deps.as_mut(), env.clone().contract.address.to_string());
        assert_eq!(balance_response.balance, add_rewards);
        assert_eq!(add_rewards_pool.attributes.get(0).unwrap().value, "add_rewards_pool");        
        assert_eq!(add_rewards_pool.attributes.get(1).unwrap().value, ADD_REWARDS_POOL.to_string());
        assert_eq!(add_rewards_pool.attributes.get(2).unwrap().value, ADD_REWARDS_POOL.to_string());

        contract_test_start(deps.as_mut(), info.clone(), env.clone(), config.clone());

        return (deps, info, env, cw721_contract, cw721_contract_address, config, staker, token_id)
    }

    fn do_instantiate(
        deps: DepsMut,
        info: MessageInfo,
        env: Env,
        white_listed_nft_contract: String,
        rewards_token_contract: String,
    ) -> Response {
        let msg = InstantiateMsg {
            cycle_length_in_seconds: CYCLE_LENGTH_IN_SECONDS,
            period_length_in_cycles: PERIOD_LENGTH_IN_CYCLES,
            white_listed_nft_contract,
            rewards_token_contract
        };
        return nft_staking_instantiate(deps, env, info, msg).unwrap();        
    }    

    fn get_config(
        deps: DepsMut,
    ) -> Result<Config, ()>{
        let config = CONFIG_STATE.load(deps.storage).unwrap();

        Ok(config)
    }

    fn contract_test_start(
        deps: DepsMut,
        info: MessageInfo,
        env: Env,
        config: Config,
    ) {
        start(deps, info, env, config).unwrap();
    }

    fn setup_contract_cw721(deps: DepsMut<'_>) -> Cw721Contract<'static, Extension, Empty, Empty, Empty> {
        let contract = Cw721Contract::default();
        let msg = Cw721BaseInstantiateMsg {
            name: CONTRACT_NAME.to_string(),
            symbol: SYMBOL.to_string(),
            minter: String::from(MINTER),
        };
        let info = mock_info("creator", &[]);
        let res = contract.instantiate(deps, mock_env_cw721(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        contract
    }

    fn setup_contract_cw20(deps: DepsMut<'_>) {
        let addr = String::from(MINTER);
        let amount = Uint128::new(200000000000);
        let minter = String::from(MINTER);
        let limit = Uint128::new(400000000000);

        let instantiate_msg = Cw20InstantiateMsg {
            name: "REWARDSCTRT".to_string(),
            symbol: "RWRD".to_string(),
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

    pub fn mock_env_cw721() -> Env {
        Env {
            block: BlockInfo {
                height: 12_345,
                time: Timestamp::from_nanos(1_571_797_419_879_305_533),
                chain_id: "cosmos-testnet-14002".to_string(),
            },
            transaction: Some(TransactionInfo { index: 3 }),
            contract: ContractInfo {
                address: Addr::unchecked("cosmos2contract_cw721"),
            },
        }
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

    pub fn test_query_rewards_token_balance(
        deps: DepsMut,
        address: String,
    ) -> BalanceResponse {
        let balance_response = query_balance(deps.as_ref(), address).unwrap();
        balance_response
    }

    fn do_stake() -> (
        OwnedDeps<MemoryStorage, MockApi, MockQuerier>,
        MessageInfo,
        Env,
        Cw721Contract<'static, Extension, Empty, Empty, Empty>,
        Addr,
        Config,
        String,
        String,
    ) {
        // test environment
        let (mut deps, info, env, cw721_contract, cw721_contract_address, config, staker, token_id) = test_environment();
        let staker_info = mock_info(staker.as_str(), &[]);
        let msg = to_binary( "send nft to stake").unwrap();
        let res = cw721_contract.send_nft(deps.as_mut(), env.clone(), staker_info.clone(), env.contract.address.clone().to_string(), token_id.clone(), msg.clone()).unwrap();

        // expected Cw721ReceiveMsg after sendNft
        let payload = Cw721ReceiveMsg {
            sender: staker.to_string(),
            token_id: token_id.clone(),
            msg,
        };
        let expected = payload.clone().into_cosmos_msg(env.contract.address.clone()).unwrap();
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

        let cw721_info = mock_info(cw721_contract_address.as_str(), &[]);
        stake_function(deps.as_mut(), cw721_info, env.clone(), timestamp, config.clone(), payload);

        return (deps, info, env, cw721_contract, cw721_contract_address, config, staker, token_id)
    }

    fn stake_function(
        mut deps: DepsMut,
        info: MessageInfo,
        env: Env,
        timestamp: u64,
        config: Config,
        msg: Cw721ReceiveMsg,
    ) {
        // total rewards pool
        let total_rewards_pool = TOTAL_REWARDS_POOL.may_load(deps.branch().storage).unwrap();
        assert_eq!(ADD_REWARDS_POOL, total_rewards_pool.unwrap());

        // balance of nft staking contract
        let address = env.contract.address.to_string();
        let balance_response = test_query_rewards_token_balance(deps.branch(), address.clone());
        assert_eq!(ADD_REWARDS_POOL, balance_response.balance.u128());

        // check rewards schedule
        let rewards_schedule = REWARDS_SCHEDULE.may_load(deps.branch().storage).unwrap();
        assert!(!rewards_schedule.is_none());

        // whitelisted nft contract only send nft
        assert_eq!(info.sender.to_string(), config.clone().white_listed_nft_contract);        

        // check started and disabled
        let start_timestamp = check_start_timestamp(deps.branch()).unwrap();
        check_disable(deps.branch()).unwrap();

        let staker = msg.sender;
        let token_id = msg.token_id;
        assert_eq!(staker, STAKER.to_string());
        assert_eq!(token_id, TOKEN_ID.to_string());

        // time stamp is temp value
        // let timestamp = env.block.time.seconds();
        let current_cycle = get_cycle(timestamp, start_timestamp, config.clone()).unwrap();
        let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());
        assert_eq!(staker_tokenid_key, STAKER.to_string().add("@").add(token_id.as_str()));

        // save staker history
        let update_histories_response = update_histories(deps.branch(), staker_tokenid_key.clone(), IS_STAKED, current_cycle).unwrap();
        assert_eq!(update_histories_response.staker, staker_tokenid_key);

        let token_infos = TOKEN_INFOS.may_load(deps.branch().storage, token_id.clone()).unwrap();
        if !token_infos.is_none() {
            // prevent duplication.
            assert!(!token_infos.clone().unwrap().is_staked);
            
            let withdraw_cycle = token_infos.unwrap().withdraw_cycle;
            // cannot re-stake when current cycle of block time is same setup withdraw cycle
            assert_ne!(current_cycle, withdraw_cycle)
        }

        let next_claims = NEXT_CLAIMS.may_load(deps.branch().storage, staker_tokenid_key.clone()).unwrap();
        if next_claims.is_none() {
            let current_period = get_period(current_cycle, config.clone()).unwrap();
            let new_next_claim = NextClaim::new(current_period, 0);            

            NEXT_CLAIMS.save(deps.branch().storage, staker_tokenid_key.clone(), &new_next_claim).unwrap();            
        }

        let new_token_info = TokenInfo::stake(staker.clone(), IS_STAKED, current_cycle);
        assert_eq!(new_token_info.owner, STAKER.to_string());
        assert!(new_token_info.is_staked);
        assert_eq!(new_token_info.bond_status, BONDED);
        
        TOKEN_INFOS.save(deps.branch().storage, token_id.clone(), &new_token_info).unwrap();        
        manage_number_nfts(deps.branch(), true);
    }

    pub fn test_unstake_function(
        mut deps: DepsMut,
        env: Env,
        info: MessageInfo,
        config: Config,
        token_id: String,
        claim_recipient_address: Option<String>,
        timestamp: u64,
    ) -> Result<Response, ContractError>{
        let staker = info.clone().sender.to_string();
        let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());
        let token_info = TokenInfo::check_staker(deps.branch(), info.clone(), token_id.clone())?;

        // compare token info in the state and check staker
        let check_token_info = TOKEN_INFOS.load(deps.branch().storage, token_id.clone())?;
        assert_eq!(token_info, check_token_info);
    
        let start_timestamp = check_start_timestamp(deps.branch())?;
        // the timestamp is temp value which is input of function
        // let timestamp = env.block.time.seconds();
        let is_staked = token_info.clone().is_staked;
    
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
    
            // check token id's bond status
            let check_token_info = TOKEN_INFOS.load(deps.branch().storage, token_id.clone())?;
            assert_eq!(check_token_info.bond_status, UNBONDING);

            return Ok(Response::new()
                .add_attribute("method", "unstake_nft")
                .add_attribute("request_unstake_time", timestamp.to_string())
                .add_attribute("bond_status", UNBONDING)
            )
        }

        check_unbonding_end(deps.as_ref(), token_info.clone(), timestamp.clone())?; 

        let current_cycle = get_cycle(timestamp, start_timestamp, config.clone())?;
        let disable = check_disable(deps.branch())?;

        let max_compute_period = MAX_COMPUTE_PERIOD.load(deps.branch().storage)?;
        let mut remain_rewards = true;
        let mut remain_rewards_value: u128 = 0;
        let mut recipient: Option<String> = Some(staker.clone());
        if !claim_recipient_address.is_none() {
            recipient = claim_recipient_address;
        }

        if !disable {
            assert!(current_cycle - token_info.clone().deposit_cycle >= 2);

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
                ).unwrap();

                if compute_reward.0.amount != 0 {
                    remain_rewards_value = remain_rewards_value + compute_reward.0.amount;
                    // next claim set last computed rewards.
                    NEXT_CLAIMS.save(deps.branch().storage, staker_tokenid_key.clone(), &compute_reward.1)?;
                } else {
                    remain_rewards = false
                }
            }
            update_histories(deps.branch(), staker_tokenid_key.clone(), !is_staked, current_cycle)?;

            let token_info = TokenInfo::unstake(!is_staked, token_info.clone().deposit_cycle, current_cycle);

            TOKEN_INFOS.save(deps.branch().storage, token_id.clone(), &token_info)?;
        }

        if remain_rewards_value != 0 {
            // for test, execute token contract trasfer
            let res = test_execute_token_contract_transfer(deps.branch(), env.clone(), info.clone(), recipient.clone().unwrap(), remain_rewards_value);
            assert_eq!(staker, res.attributes.get(2).unwrap().value);
            assert_eq!(remain_rewards_value.to_string(), res.attributes.get(3).unwrap().value);
        }

        NEXT_CLAIMS.remove(deps.branch().storage, staker_tokenid_key.clone());
        manage_number_nfts(deps.branch(), false);

        Ok(Response::new()
            .add_attribute("method", "unstake_nft")
            .add_attribute("request_unstake_time", timestamp.to_string())
            .add_attribute("claim_remain_rewards", remain_rewards_value.to_string())
            .add_attribute("recipient_remain_rewards", recipient.unwrap())
        )
    }

    fn test_execute_token_contract_transfer(
        deps: DepsMut,
        env: Env,
        _info: MessageInfo,
        recipient: String,
        amount: u128,
    ) -> Response {
        let nft_staking_contract_info = mock_info(env.contract.address.as_str(), &[]);

        let msg = Cw20ExecuteMsg::Transfer { 
            recipient: recipient, 
            amount: Uint128::from(amount)
        };
        let res = execute(deps, mock_env_cw20().clone(), nft_staking_contract_info.clone(), msg.clone()).unwrap();
        return res
    }

    fn test_execute_transfer_nft_unstake(
        deps: DepsMut,
        env: Env,
        recipient: String,
        token_id: String,
        cw721_contract: Cw721Contract<'static, Extension, Empty, Empty, Empty>,
    ) -> Response {
        let nft_staking_contract_info = mock_info(env.contract.address.as_str(), &[]);

        let res = cw721_contract.transfer_nft(deps, mock_env_cw721(), nft_staking_contract_info, recipient, token_id).unwrap();
        return res
    }

    fn claim_rewards_function(
        mut deps: DepsMut,
        info: MessageInfo,
        env: Env,
        periods: u64,
        token_id: String,
        config: Config,
        claim_recipient_address: Option<String>,
        timestamp: u64,
    ) -> Result<Response, ContractError>{
        let start_timestamp = check_start_timestamp(deps.branch()).unwrap();
        check_disable(deps.branch()).unwrap();

        let staker = info.clone().sender.to_string();
        let staker_tokenid_key = staker_tokenid_key(staker.clone(), token_id.clone());

        let token_info = TOKEN_INFOS.load(deps.branch().storage, token_id.clone())?;
        if token_info.bond_status == UNBONDING {
            return Err(ContractError::TokenIdIsUnbonding {})
        }

        let next_claim = NEXT_CLAIMS.may_load(deps.storage, staker_tokenid_key.clone()).unwrap();
        assert!(!next_claim.is_none());

        let next_claim = next_claim.unwrap();
        let now = timestamp;

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

        let contract_address = env.contract.address.to_string();

        // nft staking contract balances
        let balance_response = test_query_rewards_token_balance(deps.branch(), contract_address);
        if balance_response.balance.u128() < claim.amount {
            return Err(ContractError::InsufficientRewardsPool { 
                rewards_pool_balance: balance_response.balance.u128(), 
                claim_amount: claim.amount, 
            })
        }

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

        let mut recipient = staker;
        if !claim_recipient_address.is_none() {
            recipient = claim_recipient_address.unwrap();
        }

        Ok(test_execute_token_contract_transfer(deps.branch(), env.clone(), info.clone(), recipient, claim.amount))
    }
}