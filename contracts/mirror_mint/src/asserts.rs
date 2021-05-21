use crate::state::{AssetConfig, Position};
use cosmwasm_std::{Decimal, Deps, Env, StdError, StdResult};
use terraswap::asset::Asset;

// Check zero balance & same collateral with position
pub fn assert_collateral(
    deps: Deps,
    position: &Position,
    collateral: &Asset,
) -> StdResult<()> {
    if !collateral
        .info
        .equal(&position.collateral.info.to_normal(deps.api)?)
        || collateral.amount.is_zero()
    {
        return Err(StdError::generic_err("Wrong collateral"));
    }

    Ok(())
}

// Check zero balance & same asset with position
pub fn assert_asset(
    deps: Deps,
    position: &Position,
    asset: &Asset,
) -> StdResult<()> {
    if !asset.info.equal(&position.asset.info.to_normal(deps.api)?) || asset.amount.is_zero() {
        return Err(StdError::generic_err("Wrong asset"));
    }

    Ok(())
}

pub fn assert_migrated_asset(asset_config: &AssetConfig) -> StdResult<()> {
    if asset_config.end_price.is_some() {
        return Err(StdError::generic_err(
            "Operation is not allowed for the deprecated asset",
        ));
    }

    Ok(())
}

pub fn assert_revoked_collateral(
    load_collateral_res: (Decimal, Decimal, bool),
) -> StdResult<(Decimal, Decimal)> {
    if load_collateral_res.2 {
        return Err(StdError::generic_err(
            "The collateral asset provided is no longer valid",
        ));
    }

    Ok((load_collateral_res.0, load_collateral_res.1))
}

pub fn assert_auction_discount(auction_discount: Decimal) -> StdResult<()> {
    if auction_discount > Decimal::one() {
        Err(StdError::generic_err(
            "auction_discount must be smaller than 1",
        ))
    } else {
        Ok(())
    }
}

pub fn assert_min_collateral_ratio(min_collateral_ratio: Decimal) -> StdResult<()> {
    if min_collateral_ratio < Decimal::one() {
        Err(StdError::generic_err(
            "min_collateral_ratio must be bigger than 1",
        ))
    } else {
        Ok(())
    }
}

pub fn assert_mint_period(env: &Env, asset_config: &AssetConfig) -> StdResult<()> {
    if let Some(ipo_params) = asset_config.ipo_params.clone() {
        if ipo_params.mint_end < env.block.time.nanos() / 1_000_000_000 {
            return Err(StdError::generic_err(format!(
                "The minting period for this asset ended at time {}",
                ipo_params.mint_end
            )));
        }
    }
    Ok(())
}

pub fn assert_burn_period(env: &Env, asset_config: &AssetConfig) -> StdResult<()> {
    if let Some(ipo_params) = asset_config.ipo_params.clone() {
        if ipo_params.mint_end < env.block.time.nanos() / 1_000_000_000 {
            return Err(StdError::generic_err(format!(
                "Burning is disabled for assets with limitied minting time. Mint period ended at time {}",
                ipo_params.mint_end
            )));
        }
    }
    Ok(())
}
