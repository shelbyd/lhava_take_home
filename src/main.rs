use std::time::Duration;

use alloy::{
    eips::BlockId,
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    transports::http::reqwest::Url,
};
use anyhow::Context;
use strategy::Strategy;
use uniswap_sdk_core::{prelude::*, token};
use uniswap_v3_sdk::prelude::*;

mod strategy;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ::env_logger::builder()
        .filter(None, log::LevelFilter::Info)
        .init();

    let chain_id: u64 = env_or("CHAIN_ID", "1")?.parse()?;
    let rpc_url: Url = env_or("RPC_URL", "https://eth-mainnet.public.blastapi.io")?.parse()?;

    let mut strategy = strategy::Null;

    let provider = ProviderBuilder::new().on_http(rpc_url.clone());

    let base = token!(
        chain_id,
        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        2,
        "USDC"
    );
    let quote = Ether::on_chain(chain_id);
    // let quote = token!(
    //     chain_id,
    //     "2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599",
    //     8,
    //     "WBTC"
    // );

    let mut last_block = None;
    loop {
        let block = poll_next_block(&provider, last_block, Duration::from_secs(1)).await?;
        last_block = Some(block);
        log::info!("Block {block}");

        let block_id = BlockId::from(block);

        let pool = Pool::<EphemeralTickMapDataProvider>::from_pool_key_with_tick_data_provider(
            chain_id,
            FACTORY_ADDRESS,
            base.address(),
            quote.address(),
            FeeAmount::LOW,
            provider.clone(),
            Some(block_id),
        )
        .await?;

        let price = pool.token1_price();

        let context = strategy::TradeContext {
            price_lossy: price.to_significant(8, None)?.parse()?,
        };

        log::info!("Executing strategy with context {context:?}");
        let Some(trade) = strategy.trade(&context) else {
            log::info!("Strategy produced no trade");
            continue;
        };
        log::info!("Strategy produced {trade:?}");

        let trade = match trade {
            strategy::Trade::Buy { amount } => {
                let route = Route::new(vec![pool], base.clone(), quote.clone());
                Trade::from_route(
                    route,
                    CurrencyAmount::from_raw_amount(quote.clone(), amount)?,
                    TradeType::ExactOutput,
                )?
            }
        };

        let provider = ProviderBuilder::new().on_anvil_with_config(|anvil| {
            log::info!("Forking chain {chain_id} at {block}");
            anvil.fork(rpc_url.clone()).fork_block_number(block)
        });
        let account = provider.get_accounts().await?[0];

        let params = swap_call_parameters(
            &mut [trade],
            SwapOptions {
                recipient: account,
                ..Default::default()
            },
        )?;

        let tx = TransactionRequest::default()
            .from(account)
            .to(*SWAP_ROUTER_02_ADDRESSES
                .get(&chain_id)
                .context(format!("Unknown swap router for chain id {chain_id}"))?)
            .input(params.calldata.into())
            .value(params.value);

        let hash = provider.send_transaction(tx).await?.watch().await?;
        log::info!("Successfully executed transaction {hash}");
    }
}

fn env_or(var: &str, default: &str) -> anyhow::Result<String> {
    match std::env::var(var) {
        Ok(v) => {
            log::info!("Using provided value for {var}");
            Ok(v)
        }
        Err(std::env::VarError::NotPresent) => {
            log::info!("Env var {var} not provided, using default {default:?}");
            Ok(default.to_string())
        }
        Err(e @ std::env::VarError::NotUnicode(_)) => return Err(e.into()),
    }
}

async fn poll_next_block(
    provider: &impl alloy::providers::Provider,
    last_block: Option<u64>,
    between_calls: Duration,
) -> anyhow::Result<u64> {
    loop {
        let n = provider.get_block_number().await?;
        if Some(n) != last_block {
            return Ok(n);
        }
        tokio::time::sleep(between_calls).await;
    }
}
