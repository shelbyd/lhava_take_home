//! Example demonstrating pool creation with tick data provider and swap simulation
//!
//! # Prerequisites
//! - Environment variable MAINNET_RPC_URL must be set
//! - Requires the "extensions" feature
//!
//! # Note
//! This example uses mainnet block 17000000 for consistent results

use alloy::{
    eips::BlockId,
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    transports::http::reqwest::Url,
};
use alloy_primitives::U256;
use alloy_sol_types::SolCall;
use anyhow::Context;
use uniswap_sdk_core::{prelude::*, token};
use uniswap_v3_sdk::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ::env_logger::init();

    let chain_id: u64 = env_or("CHAIN_ID", "1")?.parse()?;
    let rpc_url: Url =
        env_or("MAINNET_RPC_URL", "https://eth-mainnet.public.blastapi.io")?.parse()?;

    let provider = ProviderBuilder::new().on_http(rpc_url);
    let block_id = BlockId::from(17000000);
    let wbtc = token!(1, "2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", 8, "WBTC");
    let weth = WETH9::on_chain(chain_id).context(format!("Unknown chain id: {chain_id}"))?;

    // Create a pool with a tick map data provider
    let pool = Pool::<EphemeralTickMapDataProvider>::from_pool_key_with_tick_data_provider(
        1,
        FACTORY_ADDRESS,
        wbtc.address(),
        weth.address(),
        FeeAmount::LOW,
        provider.clone(),
        Some(block_id),
    )
    .await?;
    // Get the output amount from the pool
    let amount_in = CurrencyAmount::from_raw_amount(wbtc.clone(), 100000000)?;
    let local_amount_out = pool.get_output_amount(&amount_in, None)?;
    let local_amount_out = local_amount_out.quotient();
    println!("Local amount out: {}", local_amount_out);

    // Get the output amount from the quoter
    let route = Route::new(vec![pool], wbtc, weth);
    let params = quote_call_parameters(&route, &amount_in, TradeType::ExactInput, None);
    let quoter = *QUOTER_ADDRESSES
        .get(&chain_id)
        .context(format!("No quoter address for chain id: {chain_id}"))?;
    let tx = TransactionRequest::default()
        .to(quoter)
        .input(params.calldata.into());
    let res = provider.call(&tx).block(block_id).await?;
    let amount_out =
        IQuoter::quoteExactInputSingleCall::abi_decode_returns(res.as_ref(), true)?.amountOut;
    println!("Quoter amount out: {}", amount_out);

    // Compare local calculation with on-chain quoter to ensure accuracy
    assert_eq!(U256::from_big_int(local_amount_out), amount_out);

    Ok(())
}

fn env_or(var: &str, default: &str) -> anyhow::Result<String> {
    match std::env::var(var) {
        Ok(v) => {
            log::info!("Using provided value for {var}");
            Ok(v)
        }
        Err(std::env::VarError::NotPresent) => {
            log::info!("Env var {var} not provided, using default {default}");
            Ok(default.to_string())
        }
        Err(e @ std::env::VarError::NotUnicode(_)) => return Err(e.into()),
    }
}
