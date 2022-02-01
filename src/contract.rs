#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Attribute, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Order, Response, StdError, StdResult, Storage, Uint128, WasmMsg,
};

use serde_json::to_string;

use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, Denom};
use cw_storage_plus::Bound;

use crate::msg::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, MasterAddressResponse, QueryMsg,
    VestingAccountResponse, VestingData, VestingSchedule,
};
use crate::state::{denom_to_key, VestingAccount, MASTER_ADDRESS, VESTING_ACCOUNTS};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let master_address = if msg.master_address.is_none() {
        info.sender.to_string()
    } else {
        msg.master_address.unwrap()
    };

    MASTER_ADDRESS.save(deps.storage, &master_address)?;
    Ok(Response::new().add_attribute("master_address", master_address.as_str()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::UpdateMasterAddress { master_address } => {
            update_master_address(deps, env, info, master_address)
        }
        ExecuteMsg::RegisterVestingAccount {
            address,
            vesting_schedule,
        } => {
            // deposit validation
            if info.funds.len() != 1 {
                return Err(StdError::generic_err("must deposit only one type of token"));
            }

            let deposit_coin = info.funds[0].clone();
            register_vesting_account(
                deps,
                env,
                info.sender.to_string(),
                address,
                Denom::Native(deposit_coin.denom),
                deposit_coin.amount,
                vesting_schedule,
            )
        }
        ExecuteMsg::DeregisterVestingAccount {
            address,
            denom,
            vested_token_recipient,
            left_vesting_token_recipient,
        } => deregister_vesting_account(
            deps,
            env,
            info,
            address,
            denom,
            vested_token_recipient,
            left_vesting_token_recipient,
        ),
        ExecuteMsg::Claim { denoms, recipient } => claim(deps, env, info, denoms, recipient),
    }
}

fn only_master(storage: &dyn Storage, sender: String) -> StdResult<()> {
    if MASTER_ADDRESS.load(storage)? != sender {
        return Err(StdError::generic_err("unauthorized"));
    }

    Ok(())
}
fn update_master_address(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    master_address: String,
) -> StdResult<Response> {
    only_master(deps.storage, info.sender.to_string())?;

    MASTER_ADDRESS.save(deps.storage, &master_address)?;
    Ok(Response::new().add_attributes(vec![
        ("action", "update_master_address"),
        ("master_address", master_address.as_str()),
    ]))
}

fn register_vesting_account(
    deps: DepsMut,
    env: Env,
    sender: String,
    recipient: String,
    deposit_denom: Denom,
    deposit_amount: Uint128,
    vesting_schedule: VestingSchedule,
) -> StdResult<Response> {
    only_master(deps.storage, sender)?;

    let denom_key = denom_to_key(deposit_denom.clone());

    // vesting_account existence check
    if VESTING_ACCOUNTS.has(deps.storage, (recipient.as_str(), &denom_key)) {
        return Err(StdError::generic_err("already exists"));
    }

    // validate vesting schedule
    vesting_schedule.validate(env.block.time.seconds(), deposit_amount)?;

    VESTING_ACCOUNTS.save(
        deps.storage,
        (recipient.as_str(), &denom_key),
        &VestingAccount {
            address: recipient.to_string(),
            vesting_denom: deposit_denom.clone(),
            vesting_amount: deposit_amount,
            vesting_schedule,
            claimed_amount: Uint128::zero(),
        },
    )?;

    Ok(Response::new().add_attributes(vec![
        ("action", "register_vesting_account"),
        ("address", recipient.as_str()),
        ("vesting_denom", &to_string(&deposit_denom).unwrap()),
        ("vesting_amount", &deposit_amount.to_string()),
    ]))
}

fn deregister_vesting_account(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    address: String,
    denom: Denom,
    vested_token_recipient: Option<String>,
    left_vesting_token_recipient: Option<String>,
) -> StdResult<Response> {
    only_master(deps.storage, info.sender.to_string())?;

    let denom_key = denom_to_key(denom.clone());
    let sender = info.sender;

    let mut messages: Vec<CosmosMsg> = vec![];

    // vesting_account existence check
    let account = VESTING_ACCOUNTS.may_load(deps.storage, (address.as_str(), &denom_key))?;
    if account.is_none() {
        return Err(StdError::generic_err(format!(
            "vesting entry is not found for denom {:?}",
            to_string(&denom).unwrap(),
        )));
    }

    let account = account.unwrap();

    // remove vesting account
    VESTING_ACCOUNTS.remove(deps.storage, (address.as_str(), &denom_key));

    let vested_amount = account
        .vesting_schedule
        .vested_amount(env.block.time.seconds())?;
    let claimed_amount = account.claimed_amount;

    // transfer already vested but not claimed amount to
    // a account address or the given `vested_token_recipient` address
    let claimable_amount = vested_amount.checked_sub(claimed_amount)?;
    if !claimable_amount.is_zero() {
        let recipient = vested_token_recipient.unwrap_or_else(|| address.to_string());
        let message: CosmosMsg = match account.vesting_denom.clone() {
            Denom::Native(denom) => BankMsg::Send {
                to_address: recipient,
                amount: vec![Coin {
                    denom,
                    amount: claimable_amount,
                }],
            }
            .into(),
            Denom::Cw20(contract_addr) => WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient,
                    amount: claimable_amount,
                })?,
                funds: vec![],
            }
            .into(),
        };

        messages.push(message);
    }

    // transfer left vesting amount to owner or
    // the given `left_vesting_token_recipient` address
    let left_vesting_amount = account.vesting_amount.checked_sub(vested_amount)?;
    if !left_vesting_amount.is_zero() {
        let recipient = left_vesting_token_recipient.unwrap_or_else(|| sender.to_string());
        let message: CosmosMsg = match account.vesting_denom.clone() {
            Denom::Native(denom) => BankMsg::Send {
                to_address: recipient,
                amount: vec![Coin {
                    denom,
                    amount: left_vesting_amount,
                }],
            }
            .into(),
            Denom::Cw20(contract_addr) => WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient,
                    amount: left_vesting_amount,
                })?,
                funds: vec![],
            }
            .into(),
        };

        messages.push(message);
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        ("action", "deregister_vesting_account"),
        ("address", address.as_str()),
        ("vesting_denom", &to_string(&account.vesting_denom).unwrap()),
        ("vesting_amount", &account.vesting_amount.to_string()),
        ("vested_amount", &vested_amount.to_string()),
        ("left_vesting_amount", &left_vesting_amount.to_string()),
    ]))
}

fn claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    denoms: Vec<Denom>,
    recipient: Option<String>,
) -> StdResult<Response> {
    let sender = info.sender;
    let recipient = recipient.unwrap_or_else(|| sender.to_string());

    let mut messages: Vec<CosmosMsg> = vec![];
    let mut attrs: Vec<Attribute> = vec![];
    for denom in denoms.iter() {
        let denom_key = denom_to_key(denom.clone());

        // vesting_account existence check
        let account = VESTING_ACCOUNTS.may_load(deps.storage, (sender.as_str(), &denom_key))?;
        if account.is_none() {
            return Err(StdError::generic_err(format!(
                "vesting entry is not found for denom {}",
                to_string(&denom).unwrap(),
            )));
        }

        let mut account = account.unwrap();
        let vested_amount = account
            .vesting_schedule
            .vested_amount(env.block.time.seconds())?;
        let claimed_amount = account.claimed_amount;

        let claimable_amount = vested_amount.checked_sub(claimed_amount)?;
        if claimable_amount.is_zero() {
            continue;
        }

        account.claimed_amount = vested_amount;
        if account.claimed_amount == account.vesting_amount {
            VESTING_ACCOUNTS.remove(deps.storage, (sender.as_str(), &denom_key));
        } else {
            VESTING_ACCOUNTS.save(deps.storage, (sender.as_str(), &denom_key), &account)?;
        }

        let message: CosmosMsg = match account.vesting_denom.clone() {
            Denom::Native(denom) => BankMsg::Send {
                to_address: recipient.clone(),
                amount: vec![Coin {
                    denom,
                    amount: claimable_amount,
                }],
            }
            .into(),
            Denom::Cw20(contract_addr) => WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: recipient.clone(),
                    amount: claimable_amount,
                })?,
                funds: vec![],
            }
            .into(),
        };

        messages.push(message);
        attrs.extend(
            vec![
                Attribute::new("vesting_denom", &to_string(&account.vesting_denom).unwrap()),
                Attribute::new("vesting_amount", &account.vesting_amount.to_string()),
                Attribute::new("vested_amount", &vested_amount.to_string()),
                Attribute::new("claim_amount", &claimable_amount.to_string()),
            ]
            .into_iter(),
        );
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![("action", "claim"), ("address", sender.as_str())])
        .add_attributes(attrs))
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    let amount = cw20_msg.amount;
    let sender = cw20_msg.sender;
    let contract = info.sender;

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::RegisterVestingAccount {
            address,
            vesting_schedule,
        }) => register_vesting_account(
            deps,
            env,
            sender,
            address,
            Denom::Cw20(contract),
            amount,
            vesting_schedule,
        ),
        Err(_) => Err(StdError::generic_err("invalid cw20 hook message")),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::MasterAddress {} => to_binary(&master_address(deps, env)?),
        QueryMsg::VestingAccount {
            address,
            start_after,
            limit,
        } => to_binary(&vesting_account(deps, env, address, start_after, limit)?),
    }
}

fn master_address(deps: Deps, _env: Env) -> StdResult<MasterAddressResponse> {
    let master_address = MASTER_ADDRESS.load(deps.storage)?;
    Ok(MasterAddressResponse { master_address })
}

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
fn vesting_account(
    deps: Deps,
    env: Env,
    address: String,
    start_after: Option<Denom>,
    limit: Option<u32>,
) -> StdResult<VestingAccountResponse> {
    let mut vestings: Vec<VestingData> = vec![];
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;

    for item in VESTING_ACCOUNTS
        .prefix(address.as_str())
        .range(
            deps.storage,
            start_after
                .map(denom_to_key)
                .map(|v| v.as_bytes().to_vec())
                .map(Bound::Exclusive),
            None,
            Order::Ascending,
        )
        .take(limit)
    {
        let (_, account) = item?;
        let vested_amount = account
            .vesting_schedule
            .vested_amount(env.block.time.seconds())?;

        vestings.push(VestingData {
            vesting_denom: account.vesting_denom,
            vesting_amount: account.vesting_amount,
            vested_amount,
            vesting_schedule: account.vesting_schedule,
            claimable_amount: vested_amount.checked_sub(account.claimed_amount)?,
        })
    }

    Ok(VestingAccountResponse { address, vestings })
}
