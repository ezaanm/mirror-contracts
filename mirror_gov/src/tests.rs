#[cfg(test)]
mod tests {
    use crate::contract::{handle, init, query, VOTING_TOKEN};
    use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
    use crate::msg::{
        Cw20HookMsg, ExecuteMsg, HandleMsg, InitMsg, PollResponse, QueryMsg, StakeResponse,
    };
    use crate::state::{config_read, State};
    use cosmwasm_std::testing::{mock_env, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{
        coins, from_binary, log, to_binary, Api, Coin, CosmosMsg, Env, Extern, HandleResponse,
        HumanAddr, StdError, Uint128, WasmMsg,
    };
    use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};

    const DEFAULT_END_HEIGHT: u64 = 100800u64;
    const TEST_CREATOR: &str = "creator";
    const TEST_VOTER: &str = "voter1";
    const TEST_VOTER_2: &str = "voter2";

    fn mock_init(mut deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>) {
        let msg = InitMsg {
            mirror_token: HumanAddr::from(VOTING_TOKEN),
        };

        let env = mock_env(TEST_CREATOR, &[]);
        let _res = init(&mut deps, env, msg).expect("contract successfully handles InitMsg");
    }

    fn mock_env_height(sender: &str, sent: &[Coin], height: u64, time: u64) -> Env {
        let mut env = mock_env(sender, sent);
        env.block.height = height;
        env.block.time = time;
        env
    }

    fn init_msg() -> InitMsg {
        InitMsg {
            mirror_token: HumanAddr::from(VOTING_TOKEN),
        }
    }

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(20, &[]);

        let msg = init_msg();
        let env = mock_env(TEST_CREATOR, &coins(2, VOTING_TOKEN));
        let res = init(&mut deps, env, msg).unwrap();
        assert_eq!(0, res.messages.len());

        let state = config_read(&mut deps.storage).load().unwrap();
        assert_eq!(
            state,
            State {
                mirror_token: deps
                    .api
                    .canonical_address(&HumanAddr::from(VOTING_TOKEN))
                    .unwrap(),
                owner: deps
                    .api
                    .canonical_address(&HumanAddr::from(TEST_CREATOR))
                    .unwrap(),
                contract_addr: deps
                    .api
                    .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                    .unwrap(),
                poll_count: 0,
                total_share: Uint128::zero(),
            }
        );
    }

    #[test]
    fn poll_not_found() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);

        let res = query(&deps, QueryMsg::Poll { poll_id: 1 });

        match res {
            Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Poll does not exist"),
            Err(e) => panic!("Unexpected error: {:?}", e),
            _ => panic!("Must return error"),
        }
    }

    #[test]
    fn fails_create_poll_invalid_quorum_percentage() {
        let mut deps = mock_dependencies(20, &[]);
        let env = mock_env("voter", &coins(11, VOTING_TOKEN));

        let msg = create_poll_msg(101, "test".to_string(), None, None, None);

        let res = handle(&mut deps, env, msg);

        match res {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => {
                assert_eq!(msg, "quorum_percentage must be 0 to 100")
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn fails_create_poll_invalid_description() {
        let mut deps = mock_dependencies(20, &[]);
        let env = mock_env(TEST_VOTER, &coins(11, VOTING_TOKEN));

        let msg = create_poll_msg(30, "a".to_string(), None, None, None);

        match handle(&mut deps, env.clone(), msg) {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Description too short"),
            Err(_) => panic!("Unknown error"),
        }

        let msg = create_poll_msg(
            100,
            "01234567890123456789012345678901234567890123456789012345678901234".to_string(),
            None,
            None,
            None,
        );

        match handle(&mut deps, env.clone(), msg) {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Description too long"),
            Err(_) => panic!("Unknown error"),
        }
    }

    fn create_poll_msg(
        quorum_percentage: u8,
        description: String,
        start_height: Option<u64>,
        end_height: Option<u64>,
        execute_msg: Option<ExecuteMsg>,
    ) -> HandleMsg {
        let msg = HandleMsg::CreatePoll {
            quorum_percentage: Some(quorum_percentage),
            description,
            start_height,
            end_height,
            execute_msg,
        };
        msg
    }

    #[test]
    fn happy_days_create_poll() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);
        let env = mock_env_height(TEST_CREATOR, &coins(2, VOTING_TOKEN), 0, 10000);

        let quorum = 30;
        let msg = create_poll_msg(quorum, "test".to_string(), None, None, None);

        let handle_res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
        assert_create_poll_result(
            1,
            quorum,
            DEFAULT_END_HEIGHT,
            0,
            TEST_CREATOR,
            handle_res,
            &mut deps,
        );
    }

    #[test]
    fn create_poll_no_quorum() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);
        let env = mock_env_height(TEST_CREATOR, &coins(2, VOTING_TOKEN), 0, 10000);

        let quorum = 0;
        let msg = create_poll_msg(quorum, "test".to_string(), None, None, None);

        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_create_poll_result(
            1,
            quorum,
            DEFAULT_END_HEIGHT,
            0,
            TEST_CREATOR,
            handle_res,
            &mut deps,
        );
    }

    #[test]
    fn fails_end_poll_before_end_height() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);
        let env = mock_env_height(TEST_CREATOR, &coins(2, VOTING_TOKEN), 0, 10000);

        let msg = create_poll_msg(0, "test".to_string(), None, Some(10001), None);

        let handle_res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
        assert_create_poll_result(1, 0, 10001, 0, TEST_CREATOR, handle_res, &mut deps);

        let res = query(&deps, QueryMsg::Poll { poll_id: 1 }).unwrap();
        let value: PollResponse = from_binary(&res).unwrap();
        assert_eq!(Some(10001), value.end_height);

        let msg = HandleMsg::EndPoll { poll_id: 1 };

        let handle_res = handle(&mut deps, env.clone(), msg);

        match handle_res {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => {
                assert_eq!(msg, "Voting period has not expired.")
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn happy_days_end_poll() {
        const POLL_END_HEIGHT: u64 = 1000;
        const POLL_ID: u64 = 1;
        let stake_amount = 1000;

        let mut deps = mock_dependencies(20, &coins(1000, VOTING_TOKEN));
        mock_init(&mut deps);
        let mut creator_env = mock_env_height(
            TEST_CREATOR,
            &coins(2, VOTING_TOKEN),
            POLL_END_HEIGHT,
            10000,
        );

        let exec_msg_bz = to_binary(&Cw20HandleMsg::Burn {
            amount: Uint128(123),
        })
        .unwrap();
        let msg = create_poll_msg(
            0,
            "test".to_string(),
            None,
            Some(creator_env.block.height + 1),
            Some(ExecuteMsg {
                contract: HumanAddr::from(VOTING_TOKEN),
                msg: exec_msg_bz.clone(),
            }),
        );

        let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

        assert_create_poll_result(
            1,
            0,
            creator_env.block.height + 1,
            0,
            TEST_CREATOR,
            handle_res,
            &mut deps,
        );

        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(
                &HumanAddr::from(MOCK_CONTRACT_ADDR),
                &Uint128(stake_amount as u128),
            )],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(stake_amount as u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN, &[]);
        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_stake_tokens_result(stake_amount, stake_amount, Some(1), handle_res, &mut deps);

        let msg = HandleMsg::CastVote {
            poll_id: 1,
            vote: "yes".to_string(),
            share: Uint128::from(stake_amount),
        };
        let env = mock_env(TEST_VOTER, &[]);
        let handle_res = handle(&mut deps, env, msg).unwrap();

        assert_eq!(
            handle_res.log,
            vec![
                log("action", "vote_casted"),
                log("poll_id", POLL_ID),
                log("share", "1000"),
                log("voter", TEST_VOTER),
            ]
        );

        creator_env.block.height = &creator_env.block.height + 1;

        let msg = HandleMsg::EndPoll { poll_id: 1 };

        let handle_res = handle(&mut deps, creator_env.clone(), msg).unwrap();

        assert_eq!(
            handle_res.log,
            vec![
                log("action", "end_poll"),
                log("poll_id", "1"),
                log("rejected_reason", ""),
                log("passed", "true"),
            ]
        );
        assert_eq!(
            handle_res.messages,
            vec![CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(VOTING_TOKEN),
                msg: exec_msg_bz,
                send: vec![],
            })]
        );
    }

    #[test]
    fn end_poll_zero_quorum() {
        let mut deps = mock_dependencies(20, &coins(1000, VOTING_TOKEN));
        mock_init(&mut deps);
        let mut env = mock_env_height(TEST_CREATOR, &coins(2, VOTING_TOKEN), 1000, 10000);

        let msg = create_poll_msg(
            0,
            "test".to_string(),
            None,
            Some(env.block.height + 1),
            Some(ExecuteMsg {
                contract: HumanAddr::from(VOTING_TOKEN),
                msg: to_binary(&Cw20HandleMsg::Burn {
                    amount: Uint128(123),
                })
                .unwrap(),
            }),
        );

        let handle_res = handle(&mut deps, env.clone(), msg).unwrap();
        assert_create_poll_result(1, 0, 1001, 0, TEST_CREATOR, handle_res, &mut deps);
        let msg = HandleMsg::EndPoll { poll_id: 1 };
        env.block.height = &env.block.height + 2;

        let handle_res = handle(&mut deps, env.clone(), msg).unwrap();

        assert_eq!(
            handle_res.log,
            vec![
                log("action", "end_poll"),
                log("poll_id", "1"),
                log("rejected_reason", "Quorum not reached"),
                log("passed", "false"),
            ]
        );

        assert_eq!(handle_res.messages.len(), 0usize)
    }

    #[test]
    fn end_poll_quorum_rejected() {
        let mut deps = mock_dependencies(20, &coins(100, VOTING_TOKEN));
        mock_init(&mut deps);
        let mut creator_env = mock_env(TEST_CREATOR, &coins(2, VOTING_TOKEN));

        let msg = create_poll_msg(
            30,
            "test".to_string(),
            None,
            Some(creator_env.block.height + 1),
            None,
        );

        let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
        assert_eq!(
            handle_res.log,
            vec![
                log("action", "create_poll"),
                log("creator", TEST_CREATOR),
                log("poll_id", "1"),
                log("quorum_percentage", "30"),
                log("end_height", "12346"),
                log("start_height", "0"),
            ]
        );

        let stake_amount = 100;
        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(100))],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(stake_amount as u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN, &[]);
        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_stake_tokens_result(stake_amount, stake_amount, Some(1), handle_res, &mut deps);

        let msg = HandleMsg::CastVote {
            poll_id: 1,
            vote: "yes".to_string(),
            share: Uint128::from(10u128),
        };
        let env = mock_env(TEST_VOTER, &[]);
        let handle_res = handle(&mut deps, env, msg).unwrap();

        assert_eq!(
            handle_res.log,
            vec![
                log("action", "vote_casted"),
                log("poll_id", "1"),
                log("share", "10"),
                log("voter", TEST_VOTER),
            ]
        );

        let msg = HandleMsg::EndPoll { poll_id: 1 };

        creator_env.block.height = &creator_env.block.height + 2;

        let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
        assert_eq!(
            handle_res.log,
            vec![
                log("action", "end_poll"),
                log("poll_id", "1"),
                log("rejected_reason", "Quorum not reached"),
                log("passed", "false"),
            ]
        );
    }

    #[test]
    fn end_poll_nay_rejected() {
        let voter1_stake = 100;
        let voter2_stake = 1000;
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);
        let mut creator_env = mock_env(TEST_CREATOR, &coins(2, VOTING_TOKEN));

        let msg = create_poll_msg(
            10,
            "test".to_string(),
            None,
            Some(creator_env.block.height + 1),
            None,
        );

        let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
        assert_eq!(
            handle_res.log,
            vec![
                log("action", "create_poll"),
                log("creator", TEST_CREATOR),
                log("poll_id", "1"),
                log("quorum_percentage", "10"),
                log("end_height", "12346"),
                log("start_height", "0"),
            ]
        );

        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(
                &HumanAddr::from(MOCK_CONTRACT_ADDR),
                &Uint128(voter1_stake as u128),
            )],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(voter1_stake as u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN, &[]);
        let handle_res = handle(&mut deps, env, msg).unwrap();
        assert_stake_tokens_result(voter1_stake, voter1_stake, Some(1), handle_res, &mut deps);

        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(
                &HumanAddr::from(MOCK_CONTRACT_ADDR),
                &Uint128(voter2_stake as u128),
            )],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER_2),
            amount: Uint128::from(voter2_stake as u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN, &[]);
        let handle_res = handle(&mut deps, env, msg).unwrap();
        assert_stake_tokens_result(
            voter1_stake + voter2_stake,
            voter2_stake,
            Some(1),
            handle_res,
            &mut deps,
        );

        let env = mock_env(TEST_VOTER_2, &[]);
        let msg = HandleMsg::CastVote {
            poll_id: 1,
            vote: "no".to_string(),
            share: Uint128::from(voter2_stake),
        };
        let handle_res = handle(&mut deps, env, msg).unwrap();
        assert_cast_vote_success(TEST_VOTER_2, voter2_stake, 1, handle_res);

        let msg = HandleMsg::EndPoll { poll_id: 1 };

        creator_env.block.height = &creator_env.block.height + 2;
        let handle_res = handle(&mut deps, creator_env.clone(), msg.clone()).unwrap();
        assert_eq!(
            handle_res.log,
            vec![
                log("action", "end_poll"),
                log("poll_id", "1"),
                log("rejected_reason", "Threshold not reached"),
                log("passed", "false"),
            ]
        );
    }

    #[test]
    fn fails_end_poll_before_start_height() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);
        let env = mock_env_height(TEST_CREATOR, &coins(2, VOTING_TOKEN), 0, 10000);

        let start_height = 1001;
        let quorum_percentage = 30;
        let msg = create_poll_msg(
            quorum_percentage,
            "test".to_string(),
            Some(start_height),
            None,
            None,
        );

        let handle_res = handle(&mut deps, env.clone(), msg.clone()).unwrap();
        assert_create_poll_result(
            1,
            quorum_percentage,
            DEFAULT_END_HEIGHT,
            start_height,
            TEST_CREATOR,
            handle_res,
            &mut deps,
        );
        let msg = HandleMsg::EndPoll { poll_id: 1 };

        let handle_res = handle(&mut deps, env.clone(), msg);

        match handle_res {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => {
                assert_eq!(msg, "Voting period has not started.")
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn fails_cast_vote_not_enough_staked() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);
        let env = mock_env_height(TEST_CREATOR, &coins(2, VOTING_TOKEN), 0, 10000);

        let msg = create_poll_msg(0, "test".to_string(), None, None, None);

        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_create_poll_result(
            1,
            0,
            DEFAULT_END_HEIGHT,
            0,
            TEST_CREATOR,
            handle_res,
            &mut deps,
        );

        let env = mock_env(TEST_VOTER, &coins(11, VOTING_TOKEN));
        let msg = HandleMsg::CastVote {
            poll_id: 1,
            vote: "yes".to_string(),
            share: Uint128::from(1u128),
        };

        let res = handle(&mut deps, env, msg);

        match res {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => {
                assert_eq!(msg, "User does not have enough staked tokens.")
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn happy_days_cast_vote() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);

        let env = mock_env_height(TEST_CREATOR, &coins(2, VOTING_TOKEN), 0, 10000);

        let quorum_percentage = 30;

        let msg = create_poll_msg(quorum_percentage, "test".to_string(), None, None, None);

        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_create_poll_result(
            1,
            quorum_percentage,
            DEFAULT_END_HEIGHT,
            0,
            TEST_CREATOR,
            handle_res,
            &mut deps,
        );

        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(11u128))],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(11u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN, &[]);
        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_stake_tokens_result(11, 11, Some(1), handle_res, &mut deps);

        let env = mock_env(TEST_VOTER, &coins(11, VOTING_TOKEN));
        let share = 10u128;
        let msg = HandleMsg::CastVote {
            poll_id: 1,
            vote: "yes".to_string(),
            share: Uint128::from(share),
        };

        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_cast_vote_success(TEST_VOTER, share, 1, handle_res);
    }

    #[test]
    fn happy_days_withdraw_voting_tokens() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);

        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(11u128))],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(11u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN, &[]);
        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_stake_tokens_result(11, 11, None, handle_res, &mut deps);

        let state = config_read(&mut deps.storage).load().unwrap();
        assert_eq!(
            state,
            State {
                mirror_token: deps
                    .api
                    .canonical_address(&HumanAddr::from(VOTING_TOKEN))
                    .unwrap(),
                owner: deps
                    .api
                    .canonical_address(&HumanAddr::from(TEST_CREATOR))
                    .unwrap(),
                contract_addr: deps
                    .api
                    .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                    .unwrap(),
                poll_count: 0,
                total_share: Uint128::from(11u128),
            }
        );

        let env = mock_env(TEST_VOTER, &coins(11, VOTING_TOKEN));
        let msg = HandleMsg::WithdrawVotingTokens {
            amount: Some(Uint128::from(11u128)),
        };

        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        let msg = handle_res.messages.get(0).expect("no message");

        assert_eq!(
            msg,
            &CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(VOTING_TOKEN),
                msg: to_binary(&Cw20HandleMsg::Transfer {
                    recipient: HumanAddr::from(TEST_VOTER),
                    amount: Uint128::from(11u128),
                })
                .unwrap(),
                send: vec![],
            })
        );

        let state = config_read(&mut deps.storage).load().unwrap();
        assert_eq!(
            state,
            State {
                mirror_token: deps
                    .api
                    .canonical_address(&HumanAddr::from(VOTING_TOKEN))
                    .unwrap(),
                owner: deps
                    .api
                    .canonical_address(&HumanAddr::from(TEST_CREATOR))
                    .unwrap(),
                contract_addr: deps
                    .api
                    .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                    .unwrap(),
                poll_count: 0,
                total_share: Uint128::zero(),
            }
        );
    }

    #[test]
    fn fails_withdraw_voting_tokens_no_stake() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);

        let env = mock_env(TEST_VOTER, &coins(11, VOTING_TOKEN));
        let msg = HandleMsg::WithdrawVotingTokens {
            amount: Some(Uint128::from(11u128)),
        };

        let res = handle(&mut deps, env, msg);

        match res {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Nothing staked"),
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn fails_withdraw_too_many_tokens() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);

        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(10u128))],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(10u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN, &[]);
        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_stake_tokens_result(10, 10, None, handle_res, &mut deps);

        let env = mock_env(TEST_VOTER, &[]);
        let msg = HandleMsg::WithdrawVotingTokens {
            amount: Some(Uint128::from(11u128)),
        };

        let res = handle(&mut deps, env, msg);

        match res {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => {
                assert_eq!(msg, "User is trying to withdraw too many tokens.")
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn fails_cast_vote_twice() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);

        let env = mock_env_height(TEST_CREATOR, &coins(2, VOTING_TOKEN), 0, 10000);

        let quorum_percentage = 30;
        let msg = create_poll_msg(quorum_percentage, "test".to_string(), None, None, None);
        let handle_res = handle(&mut deps, env.clone(), msg.clone()).unwrap();

        assert_create_poll_result(
            1,
            quorum_percentage,
            DEFAULT_END_HEIGHT,
            0,
            TEST_CREATOR,
            handle_res,
            &mut deps,
        );

        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(11u128))],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(11u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN, &[]);
        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_stake_tokens_result(11, 11, Some(1), handle_res, &mut deps);

        let share = 1u128;
        let msg = HandleMsg::CastVote {
            poll_id: 1,
            vote: "yes".to_string(),
            share: Uint128::from(share),
        };
        let env = mock_env(TEST_VOTER, &[]);
        let handle_res = handle(&mut deps, env.clone(), msg).unwrap();
        assert_cast_vote_success(TEST_VOTER, share, 1, handle_res);

        let msg = HandleMsg::CastVote {
            poll_id: 1,
            vote: "yes".to_string(),
            share: Uint128::from(share),
        };
        let res = handle(&mut deps, env, msg);

        match res {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "User has already voted."),
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn fails_cast_vote_without_poll() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);

        let msg = HandleMsg::CastVote {
            poll_id: 0,
            vote: "yes".to_string(),
            share: Uint128::from(1u128),
        };
        let env = mock_env(TEST_VOTER, &coins(11, VOTING_TOKEN));

        let res = handle(&mut deps, env, msg);

        match res {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Poll does not exist"),
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn happy_days_stake_voting_tokens() {
        let mut deps = mock_dependencies(20, &[]);
        mock_init(&mut deps);

        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(11u128))],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(11u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN, &[]);
        let handle_res = handle(&mut deps, env, msg.clone()).unwrap();
        assert_stake_tokens_result(11, 11, None, handle_res, &mut deps);
    }

    #[test]
    fn fails_insufficient_funds() {
        let mut deps = mock_dependencies(20, &[]);

        // initialize the store
        let msg = init_msg();
        let env = mock_env(TEST_VOTER, &coins(2, VOTING_TOKEN));
        let init_res = init(&mut deps, env, msg).unwrap();
        assert_eq!(0, init_res.messages.len());

        // insufficient token
        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(0u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN, &[]);
        let res = handle(&mut deps, env, msg);

        match res {
            Ok(_) => panic!("Must return error"),
            Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "Insufficient funds sent"),
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn fails_staking_wrong_token() {
        let mut deps = mock_dependencies(20, &[]);

        // initialize the store
        let msg = init_msg();
        let env = mock_env(TEST_VOTER, &coins(2, VOTING_TOKEN));
        let init_res = init(&mut deps, env, msg).unwrap();
        assert_eq!(0, init_res.messages.len());

        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(11u128))],
        )]);

        // wrong token
        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(11u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN.to_string() + "2", &[]);
        let res = handle(&mut deps, env, msg);

        match res {
            Ok(_) => panic!("Must return error"),
            Err(StdError::Unauthorized { .. }) => {}
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn share_calculation() {
        let mut deps = mock_dependencies(20, &[]);

        // initialize the store
        let msg = init_msg();
        let env = mock_env(TEST_VOTER, &coins(2, VOTING_TOKEN));
        let init_res = init(&mut deps, env, msg).unwrap();
        assert_eq!(0, init_res.messages.len());

        // create 100 share
        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(100u128))],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(100u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN.to_string(), &[]);
        let _res = handle(&mut deps, env, msg);

        // add more balance(100) to make share:balance = 1:2
        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(
                &HumanAddr::from(MOCK_CONTRACT_ADDR),
                &Uint128(200u128 + 100u128),
            )],
        )]);

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: HumanAddr::from(TEST_VOTER),
            amount: Uint128::from(100u128),
            msg: Some(to_binary(&Cw20HookMsg::StakeVotingTokens {}).unwrap()),
        });

        let env = mock_env(VOTING_TOKEN.to_string(), &[]);
        let res = handle(&mut deps, env, msg).unwrap();
        assert_eq!(
            res.log,
            vec![
                log("action", "staking"),
                log("sender", TEST_VOTER),
                log("share", "50"),
                log("amount", "100"),
            ]
        );

        let msg = HandleMsg::WithdrawVotingTokens {
            amount: Some(Uint128(100u128)),
        };
        let env = mock_env(TEST_VOTER.to_string(), &[]);
        let res = handle(&mut deps, env, msg).unwrap();
        assert_eq!(
            res.log,
            vec![
                log("action", "withdraw"),
                log("recipient", TEST_VOTER),
                log("amount", "100"),
            ]
        );

        // 100 tokens withdrawn
        deps.querier.with_token_balances(&[(
            &HumanAddr::from(VOTING_TOKEN),
            &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(200u128))],
        )]);

        let res = query(
            &mut deps,
            QueryMsg::Stake {
                address: HumanAddr::from(TEST_VOTER),
            },
        )
        .unwrap();
        let stake_info: StakeResponse = from_binary(&res).unwrap();
        assert_eq!(stake_info.share, Uint128(100));
        assert_eq!(stake_info.balance, Uint128(200));
    }

    // helper to confirm the expected create_poll response
    fn assert_create_poll_result(
        poll_id: u64,
        quorum: u8,
        end_height: u64,
        start_height: u64,
        creator: &str,
        handle_res: HandleResponse,
        deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
    ) {
        assert_eq!(
            handle_res.log,
            vec![
                log("action", "create_poll"),
                log("creator", creator),
                log("poll_id", poll_id.to_string()),
                log("quorum_percentage", quorum.to_string()),
                log("end_height", end_height.to_string()),
                log("start_height", start_height.to_string()),
            ]
        );

        //confirm poll count
        let state = config_read(&deps.storage).load().unwrap();
        assert_eq!(
            state,
            State {
                mirror_token: deps
                    .api
                    .canonical_address(&HumanAddr::from(VOTING_TOKEN))
                    .unwrap(),
                owner: deps
                    .api
                    .canonical_address(&HumanAddr::from(TEST_CREATOR))
                    .unwrap(),
                contract_addr: deps
                    .api
                    .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                    .unwrap(),
                poll_count: 1,
                total_share: Uint128::zero(),
            }
        );
    }

    fn assert_stake_tokens_result(
        total_share: u128,
        new_share: u128,
        poll_count: Option<u64>,
        handle_res: HandleResponse,
        deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
    ) {
        assert_eq!(
            handle_res.log.get(2).expect("no log"),
            &log("share", new_share.to_string())
        );

        let state = config_read(&mut deps.storage).load().unwrap();
        assert_eq!(
            state,
            State {
                mirror_token: deps
                    .api
                    .canonical_address(&HumanAddr::from(VOTING_TOKEN))
                    .unwrap(),
                owner: deps
                    .api
                    .canonical_address(&HumanAddr::from(TEST_CREATOR))
                    .unwrap(),
                contract_addr: deps
                    .api
                    .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
                    .unwrap(),
                poll_count: poll_count.unwrap_or_default(),
                total_share: Uint128::from(total_share),
            }
        );
    }

    fn assert_cast_vote_success(
        voter: &str,
        share: u128,
        poll_id: u64,
        handle_res: HandleResponse,
    ) {
        assert_eq!(
            handle_res.log,
            vec![
                log("action", "vote_casted"),
                log("poll_id", poll_id.to_string()),
                log("share", share.to_string()),
                log("voter", voter),
            ]
        );
    }
}
