## Token Vesting

This contract is to provide vesting account feature for the both cw20 and native tokens, which is controlled by a master address.

### Instantiate Contract
If master address is not given, the instantiator address will be used as master address

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InstantiateMsg {
    pub master_address: Option<String>,
}
```

### Master Operations
All accounts can be registered and de-registered only from a master address.

* UpdateMasterAddress - update master address to a new address
* RegisterVestingAccount   - register vesting account
  * When creating vesting account, the one can specify the `master_address` to enable deregister feature.
* DeregisterVestingAccount  - deregister vesting account
  * This interface only executable from the `master_address` of a vesting account.
  * It will compute `claimable_amount` and `left_vesting_amount`. Each amount respectively sent to (`vested_token_recipient` or `vesting_account`) and (`left_vesting_token_recipient` or `master_address`).

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
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
```

### Vesting Account Operations

* Claim - send newly vested token to the (`recipient` or `vesting_account`). The `claim_amount` is computed as (`vested_amount` - `claimed_amount`) and `claimed_amount` is updated to `vested_amount`.

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    ////////////////////////
    /// VestingAccount Operations ///
    ////////////////////////
    Claim {
        denoms: Vec<Denom>,
        recipient: Option<String>,
    },
}
```

### Deployed Contract Info
| data          | bombay-12 | columbus-5 |
| ------------- | --------- | ---------- |
| code_id       | N/A       | N/A        |
| contract_addr | N/A       | N/A        |
