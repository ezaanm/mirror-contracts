use cosmwasm_std::{
    Api, Binary, CanonicalAddr, Extern, HumanAddr, Querier, QueryRequest, StdResult, Storage,
    Uint128, WasmQuery,
};

use cosmwasm_storage::to_length_prefixed;

pub fn load_token_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: &HumanAddr,
    account_addr: &CanonicalAddr,
) -> StdResult<Uint128> {
    // load balance form the token contract
    let balance: Uint128 = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: HumanAddr::from(contract_addr),
            key: Binary::from(concat(
                &to_length_prefixed(b"balances").to_vec(),
                account_addr.as_slice(),
            )),
        }))
        .unwrap_or_else(|_| Uint128::zero());

    Ok(balance)
}

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}
