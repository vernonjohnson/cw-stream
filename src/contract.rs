use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ReceiveMsg, StreamResponse,
};
use crate::state::{save_stream, Config, Stream, CONFIG, STREAMS, STREAM_SEQ};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
};
use cw2::set_contract_version;
use cw20::{Cw20Contract, Cw20ExecuteMsg, Cw20ReceiveMsg};

const CONTRACT_NAME: &str = "crates.io:cw-stream";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let owner = msg
        .owner
        .and_then(|s| deps.api.addr_validate(s.as_str()).ok())
        .unwrap_or(info.sender);
    let config = Config {
        owner: owner.clone(),
        cw20_addr: deps.api.addr_validate(msg.cw20_addr.as_str())?,
    };
    CONFIG.save(deps.storage, &config)?;

    STREAM_SEQ.save(deps.storage, &Uint128::new(0))?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", owner)
        .add_attribute("cw20_addr", msg.cw20_addr))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => execute_receive(env, deps, info, msg),
        ExecuteMsg::Withdraw { id } => try_withdraw(env, deps, info, id),
    }
}

pub fn try_withdraw(
    env: Env,
    deps: DepsMut,
    info: MessageInfo,
    id: Uint128,
) -> Result<Response, ContractError> {
    let mut stream = STREAMS.load(deps.storage, id.u128().into())?;
    if stream.recipient != info.sender {
        return Err(ContractError::NotStreamRecipient {});
    }

    if stream.claimed_amount >= stream.amount {
        return Err(ContractError::StreamFullyClaimed {});
    }

    if stream.claimed_amount >= stream.amount {
        return Err(ContractError::StreamFullyClaimed {});
    }

    let block_time = env.block.time.nanos() / 1_000_000;
    if stream.start_time < block_time {
        return Err(ContractError::StreamNotStarted {});
    }

    let block_time = Uint128::from(block_time);
    let start_time = Uint128::from(stream.start_time);
    let end_time = Uint128::from(stream.end_time);

    let claimable_amount = ((block_time - start_time) / (end_time - start_time) * stream.amount)
        - stream.claimed_amount;
    if claimable_amount < Uint128::new(0) {
        return Err(ContractError::NoFundsToClaim {});
    }

    stream.claimed_amount += claimable_amount;
    STREAMS.save(deps.storage, id.u128().into(), &stream)?;

    let config = CONFIG.load(deps.storage)?;
    let cw20 = Cw20Contract(config.cw20_addr);
    let msg = cw20.call(Cw20ExecuteMsg::Transfer {
        recipient: stream.recipient.to_string(),
        amount: claimable_amount,
    })?;

    let res = Response::new()
        .add_attribute("method", "withdraw")
        .add_attribute("stream_id", id)
        .add_attribute("amount", claimable_amount)
        .add_attribute("recipient", stream.recipient.to_string())
        .add_message(msg);
    Ok(res)
}

pub fn try_create_stream(
    env: Env,
    deps: DepsMut,
    owner: String,
    recipient: String,
    amount: Uint128,
    start_time: u64,
    end_time: u64,
) -> Result<Response, ContractError> {
    if start_time > end_time {
        return Err(ContractError::InvalidStartTime {});
    }

    let block_time = env.block.time.nanos() / 1_000_000;
    if start_time < block_time {
        return Err(ContractError::InvalidStartTime {});
    }

    let validated_owner = deps.api.addr_validate(owner.as_str())?;
    assert_eq!(validated_owner, owner);

    let validated_recipient = deps.api.addr_validate(recipient.as_str())?;
    assert_eq!(validated_recipient, recipient);

    let stream = Stream {
        owner: validated_owner,
        recipient: validated_recipient,
        amount,
        claimed_amount: Uint128::zero(),
        start_time,
        end_time,
    };

    save_stream(deps, &stream)?;

    Ok(Response::new()
        .add_attribute("method", "try_create_stream")
        .add_attribute("owner", owner)
        .add_attribute("recipient", recipient)
        .add_attribute("amount", amount)
        .add_attribute("start_time", start_time.to_string())
        .add_attribute("end_time", end_time.to_string()))
}

pub fn execute_receive(
    env: Env,
    deps: DepsMut,
    info: MessageInfo,
    wrapped: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if config.cw20_addr != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let msg: ReceiveMsg = from_binary(&wrapped.msg)?;
    match msg {
        ReceiveMsg::CreateStream {
            recipient,
            start_time,
            end_time,
        } => try_create_stream(
            env,
            deps,
            wrapped.sender,
            recipient,
            wrapped.amount,
            start_time,
            end_time,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::GetStream { id } => to_binary(&query_stream(deps, id)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: config.owner.into_string(),
        cw20_addr: config.cw20_addr.into_string(),
    })
}

fn query_stream(deps: Deps, id: Uint128) -> StdResult<StreamResponse> {
    let stream = STREAMS.load(deps.storage, id.u128().into())?;
    Ok(StreamResponse {
        owner: stream.owner.into_string(),
        recipient: stream.recipient.into_string(),
        amount: stream.amount,
        claimed_amount: stream.claimed_amount,
        start_time: stream.start_time,
        end_time: stream.end_time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{attr, Addr};

    #[test]
    fn initialization() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            owner: None,
            cw20_addr: String::from(MOCK_CONTRACT_ADDR),
        };
        let info = mock_info("creator", &[]);

        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(_res.attributes[0], attr("method", "instantiate"));
        assert_eq!(_res.attributes[1], attr("owner", "creator"));
        assert_eq!(
            _res.attributes[2],
            attr("cw20_addr", String::from(MOCK_CONTRACT_ADDR))
        );
    }

    #[test]
    fn create_stream() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            owner: None,
            cw20_addr: String::from(MOCK_CONTRACT_ADDR),
        };
        let mut info = mock_info("Alice", &[]);
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let sender = Addr::unchecked("Alice").to_string();
        let recipient = Addr::unchecked("Bob").to_string();
        let amount = Uint128::new(100);
        let start_time = mock_env().block.time.plus_seconds(10000).nanos() / 1_000_000;
        let end_time = mock_env().block.time.plus_seconds(15000).nanos() / 1_000_000;

        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: sender.clone(),
            amount: amount.clone(),
            msg: to_binary(&ReceiveMsg::CreateStream {
                recipient: recipient.clone(),
                start_time: start_time.clone(),
                end_time: end_time.clone(),
            })
            .unwrap(),
        });
        info.sender = Addr::unchecked(MOCK_CONTRACT_ADDR);
        let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(res.attributes[0], attr("method", "try_create_stream"));
        assert_eq!(res.attributes[1], attr("owner", sender));
        assert_eq!(res.attributes[2], attr("recipient", recipient));
        assert_eq!(res.attributes[3], attr("amount", amount));
        assert_eq!(
            res.attributes[4],
            attr("start_time", start_time.to_string())
        );
        assert_eq!(res.attributes[5], attr("end_time", end_time.to_string()));

        let msg = QueryMsg::GetStream {
            id: Uint128::new(1),
        };
        let res = query(deps.as_ref(), mock_env(), msg).unwrap();
        let stream: Stream = from_binary(&res).unwrap();
        assert_eq!(
            stream,
            Stream {
                owner: Addr::unchecked("Alice"),
                recipient: Addr::unchecked("Bob"),
                amount: amount.clone(),
                claimed_amount: Uint128::new(0),
                start_time: start_time.clone(),
                end_time: end_time.clone()
            }
        );
    }

    #[test]
    fn invalid_start_time() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            owner: None,
            cw20_addr: String::from(MOCK_CONTRACT_ADDR),
        };
        let mut info = mock_info("Alice", &[]);
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let sender = Addr::unchecked("Alice").to_string();
        let recipient = Addr::unchecked("Bob").to_string();
        let amount = Uint128::new(100);
        let start_time = mock_env().block.time.plus_seconds(10000).nanos() / 1_000_000;
        let end_time = mock_env().block.time.plus_seconds(2000).nanos() / 1_000_000;

        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: sender.clone(),
            amount: amount.clone(),
            msg: to_binary(&ReceiveMsg::CreateStream {
                recipient: recipient.clone(),
                start_time: start_time.clone(),
                end_time: end_time.clone(),
            })
            .unwrap(),
        });
        info.sender = Addr::unchecked(MOCK_CONTRACT_ADDR);
        let err = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();

        match err {
            ContractError::InvalidStartTime {} => {}
            e => panic!("unexpected error: {}", e),
        }
    }

    #[test]
    fn invalid_cw20_addr() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            owner: None,
            cw20_addr: String::from(MOCK_CONTRACT_ADDR),
        };
        let mut info = mock_info("Alice", &[]);
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let sender = Addr::unchecked("Alice").to_string();
        let recipient = Addr::unchecked("Bob").to_string();
        let amount = Uint128::new(100);
        let start_time = mock_env().block.time.plus_seconds(10000).nanos() / 1_000_000;
        let end_time = mock_env().block.time.plus_seconds(2000).nanos() / 1_000_000;

        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: sender.clone(),
            amount: amount.clone(),
            msg: to_binary(&ReceiveMsg::CreateStream {
                recipient: recipient.clone(),
                start_time: start_time.clone(),
                end_time: end_time.clone(),
            })
            .unwrap(),
        });
        info.sender = Addr::unchecked("wrongCw20");
        let err = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();

        match err {
            ContractError::Unauthorized {} => {}
            e => panic!("unexpected error: {}", e),
        }
    }

    #[test]
    fn withdraw() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            owner: None,
            cw20_addr: String::from(MOCK_CONTRACT_ADDR),
        };
        let mut info = mock_info("Alice", &[]);
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let sender = Addr::unchecked("Alice").to_string();
        let recipient = Addr::unchecked("Bob").to_string();
        let amount = Uint128::new(100);
        let start_time = mock_env().block.time.nanos() / 1_000_000;
        let end_time = mock_env().block.time.plus_seconds(15000).nanos() / 1_000_000;

        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: sender.clone(),
            amount: amount.clone(),
            msg: to_binary(&ReceiveMsg::CreateStream {
                recipient: recipient.clone(),
                start_time: start_time.clone(),
                end_time: end_time.clone(),
            })
            .unwrap(),
        });
        info.sender = Addr::unchecked(MOCK_CONTRACT_ADDR);
        let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(res.attributes[0], attr("method", "try_create_stream"));
        assert_eq!(res.attributes[1], attr("owner", sender));
        assert_eq!(res.attributes[2], attr("recipient", recipient));
        assert_eq!(res.attributes[3], attr("amount", amount));
        assert_eq!(
            res.attributes[4],
            attr("start_time", start_time.to_string())
        );
        assert_eq!(res.attributes[5], attr("end_time", end_time.to_string()));

        let msg = QueryMsg::GetStream {
            id: Uint128::new(1),
        };
        let res = query(deps.as_ref(), mock_env(), msg).unwrap();
        let stream: Stream = from_binary(&res).unwrap();
        assert_eq!(
            stream,
            Stream {
                owner: Addr::unchecked("Alice"),
                recipient: Addr::unchecked("Bob"),
                amount: amount.clone(),
                claimed_amount: Uint128::new(0),
                start_time: start_time.clone(),
                end_time: end_time.clone()
            }
        );

        let msg = ExecuteMsg::Withdraw {
            id: Uint128::new(1),
        };

        info.sender = Addr::unchecked("Bob");
        let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(res.attributes[0], attr("method", "withdraw"));
        assert_eq!(res.attributes[1], attr("stream_id", Uint128::new(1)));
        // TODO: Assertion for claimed amount
        assert_eq!(res.attributes[3], attr("recipient", Addr::unchecked("Bob")));
    }
}
