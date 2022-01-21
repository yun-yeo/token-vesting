use cosmwasm_std::{StdError, StdResult, Uint128};
use cw20::{Cw20ReceiveMsg, Denom};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InstantiateMsg {
    pub master_address: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),

    //////////////////////////
    /// Creator Operations ///
    //////////////////////////
    RegisterVestingAccount {
        address: String,
        vesting_schedule: VestingSchedule,
    },
    /// only available when master_address was set
    DeregisterVestingAccount {
        address: String,
        denom: Denom,
        vested_token_recipient: Option<String>,
        left_vesting_token_recipient: Option<String>,
    },
    UpdateMasterAddress {
        master_address: String,
    },

    ////////////////////////
    /// VestingAccount Operations ///
    ////////////////////////
    Claim {
        denoms: Vec<Denom>,
        recipient: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Register vesting account with token transfer
    RegisterVestingAccount {
        address: String,
        vesting_schedule: VestingSchedule,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    MasterAddress {},
    VestingAccount {
        address: String,
        start_after: Option<Denom>,
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Debug)]
pub struct MasterAddressResponse {
    pub master_address: String,
}

#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Debug)]
pub struct VestingAccountResponse {
    pub address: String,
    pub vestings: Vec<VestingData>,
}

#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Debug)]
pub struct VestingData {
    pub vesting_denom: Denom,
    pub vesting_amount: Uint128,
    pub vested_amount: Uint128,
    pub vesting_schedule: VestingSchedule,
    pub claimable_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VestingSchedule {
    /// LinearVesting is used to vest tokens linearly during a time period.
    /// The total_amount will be vested during this period.
    LinearVesting {
        start_time: String,      // vesting start time in second unit
        end_time: String,        // vesting end time in second unit
        vesting_amount: Uint128, // total vesting amount
    },
    /// PeriodicVesting is used to vest tokens
    /// at regular intervals for a specific period.
    /// To minimize calculation error,
    /// (end_time - start_time) should be multiple of vesting_interval
    /// deposit_amount = amount * ((end_time - start_time) / vesting_interval + 1)
    PeriodicVesting {
        start_time: String,       // vesting start time in second unit
        end_time: String,         // vesting end time in second unit
        vesting_interval: String, // vesting interval in second unit
        amount: Uint128,          // the amount will be vested in a interval
    },
    /// CliffVesting is used to vest tokens
    /// according to a predefined schedules vector.
    /// The deposit token must be equal with sum of all schedules.
    CliffVesting { schedules: Vec<CliffSchedule> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct CliffSchedule {
    pub release_time: String,
    pub release_amount: Uint128,
}

impl VestingSchedule {
    pub fn validate(&self, block_time: u64, deposit_amount: Uint128) -> StdResult<()> {
        if deposit_amount.is_zero() {
            return Err(StdError::generic_err("assert(deposit_amount > 0)"));
        }

        match self {
            VestingSchedule::LinearVesting {
                start_time,
                end_time,
                vesting_amount,
            } => {
                if vesting_amount.is_zero() {
                    return Err(StdError::generic_err("assert(vesting_amount > 0)"));
                }

                let start_time = start_time
                    .parse::<u64>()
                    .map_err(|_| StdError::generic_err("invalid start_time"))?;
                let end_time = end_time
                    .parse::<u64>()
                    .map_err(|_| StdError::generic_err("invalid end_time"))?;
                if start_time < block_time {
                    return Err(StdError::generic_err("assert(start_time >= block_time)"));
                }
                if end_time < start_time {
                    return Err(StdError::generic_err("assert(end_time >= start_time)"));
                }
                if vesting_amount.u128() != deposit_amount.u128() {
                    return Err(StdError::generic_err(
                        "assert(deposit_amount == vesting_amount)",
                    ));
                }
            }
            VestingSchedule::PeriodicVesting {
                start_time,
                end_time,
                vesting_interval,
                amount,
            } => {
                if amount.is_zero() {
                    return Err(StdError::generic_err("assert(vesting_amount > 0)"));
                }

                let start_time = start_time
                    .parse::<u64>()
                    .map_err(|_| StdError::generic_err("invalid start_time"))?;
                let end_time = end_time
                    .parse::<u64>()
                    .map_err(|_| StdError::generic_err("invalid end_time"))?;
                let vesting_interval = vesting_interval
                    .parse::<u64>()
                    .map_err(|_| StdError::generic_err("invalid vesting_interval"))?;
                if start_time < block_time {
                    return Err(StdError::generic_err("start_time >= block_time"));
                }
                if end_time < start_time {
                    return Err(StdError::generic_err("assert(end_time >= start_time)"));
                }
                if vesting_interval == 0 {
                    return Err(StdError::generic_err("assert(vesting_interval != 0)"));
                }
                let time_period = end_time - start_time;
                if time_period != (time_period / vesting_interval) * vesting_interval {
                    return Err(StdError::generic_err(
                        "assert((end_time - start_time) % vesting_interval == 0)",
                    ));
                }
                let num_interval = 1 + time_period / vesting_interval;
                let vesting_amount = amount.checked_mul(Uint128::from(num_interval))?;
                if vesting_amount != deposit_amount {
                    return Err(StdError::generic_err(
                        "assert(deposit_amount = amount * ((end_time - start_time) / vesting_interval + 1))",
                    ));
                }
            }
            VestingSchedule::CliffVesting { schedules } => {
                if schedules.len() == 0 {
                    return Err(StdError::generic_err("assert(schedules.len() > 0)"));
                }

                let mut vesting_amount = Uint128::zero();
                for schedule in schedules.iter() {
                    if schedule.release_amount.is_zero() {
                        return Err(StdError::generic_err("assert(release_amount > 0)"));
                    }

                    let release_time = schedule.release_time.parse::<u64>().unwrap();
                    if release_time < block_time {
                        return Err(StdError::generic_err("release_time >= block_time"));
                    }

                    vesting_amount = vesting_amount.checked_add(schedule.release_amount)?;
                }

                if deposit_amount.u128() != vesting_amount.u128() {
                    return Err(StdError::generic_err(
                        "assert(deposit_amount == vesting_amount)",
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn vested_amount(&self, block_time: u64) -> StdResult<Uint128> {
        match self {
            VestingSchedule::LinearVesting {
                start_time,
                end_time,
                vesting_amount,
            } => {
                let start_time = start_time.parse::<u64>().unwrap();
                let end_time = end_time.parse::<u64>().unwrap();

                if block_time <= start_time {
                    return Ok(Uint128::zero());
                }

                if block_time >= end_time {
                    return Ok(*vesting_amount);
                }

                let vested_token = vesting_amount
                    .checked_mul(Uint128::from(block_time - start_time))?
                    .checked_div(Uint128::from(end_time - start_time))?;

                Ok(vested_token)
            }
            VestingSchedule::PeriodicVesting {
                start_time,
                end_time,
                vesting_interval,
                amount,
            } => {
                let start_time = start_time.parse::<u64>().unwrap();
                let end_time = end_time.parse::<u64>().unwrap();
                let vesting_interval = vesting_interval.parse::<u64>().unwrap();

                if block_time < start_time {
                    return Ok(Uint128::zero());
                }

                let num_interval = 1 + (end_time - start_time) / vesting_interval;
                if block_time >= end_time {
                    return Ok(amount.checked_mul(Uint128::from(num_interval))?);
                }

                let passed_interval = 1 + (block_time - start_time) / vesting_interval;
                Ok(amount.checked_mul(Uint128::from(passed_interval))?)
            }
            VestingSchedule::CliffVesting { schedules } => Ok(Uint128::new(
                schedules
                    .iter()
                    .map(|s| {
                        let release_time = s.release_time.parse::<u64>().unwrap();
                        if block_time >= release_time {
                            s.release_amount.u128()
                        } else {
                            0u128
                        }
                    })
                    .sum(),
            )),
        }
    }
}

#[test]
fn linear_vesting_vested_amount() {
    let schedule = VestingSchedule::LinearVesting {
        start_time: "100".to_string(),
        end_time: "110".to_string(),
        vesting_amount: Uint128::new(1000000u128),
    };

    assert_eq!(schedule.vested_amount(100).unwrap(), Uint128::zero());
    assert_eq!(
        schedule.vested_amount(105).unwrap(),
        Uint128::new(500000u128)
    );
    assert_eq!(
        schedule.vested_amount(110).unwrap(),
        Uint128::new(1000000u128)
    );
    assert_eq!(
        schedule.vested_amount(115).unwrap(),
        Uint128::new(1000000u128)
    );
}

#[test]
fn periodic_vesting_vested_amount() {
    let schedule = VestingSchedule::PeriodicVesting {
        start_time: "105".to_string(),
        end_time: "110".to_string(),
        vesting_interval: "5".to_string(),
        amount: Uint128::new(500000u128),
    };

    assert_eq!(schedule.vested_amount(100).unwrap(), Uint128::zero());
    assert_eq!(
        schedule.vested_amount(105).unwrap(),
        Uint128::new(500000u128)
    );
    assert_eq!(
        schedule.vested_amount(110).unwrap(),
        Uint128::new(1000000u128)
    );
    assert_eq!(
        schedule.vested_amount(115).unwrap(),
        Uint128::new(1000000u128)
    );
}

#[test]
fn cliff_vesting_vested_amount() {
    let schedule = VestingSchedule::CliffVesting {
        schedules: vec![
            CliffSchedule {
                release_time: "105".to_string(),
                release_amount: Uint128::new(500000u128),
            },
            CliffSchedule {
                release_time: "110".to_string(),
                release_amount: Uint128::new(500000u128),
            },
        ],
    };

    assert_eq!(schedule.vested_amount(100).unwrap(), Uint128::zero());
    assert_eq!(
        schedule.vested_amount(105).unwrap(),
        Uint128::new(500000u128)
    );
    assert_eq!(
        schedule.vested_amount(110).unwrap(),
        Uint128::new(1000000u128)
    );
    assert_eq!(
        schedule.vested_amount(115).unwrap(),
        Uint128::new(1000000u128)
    );
}
