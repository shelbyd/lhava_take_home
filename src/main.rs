use std::time::Duration;

use alloy::{
    eips::BlockId,
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    transports::http::reqwest::Url,
};
use anyhow::Context;
use uniswap_sdk_core::{prelude::*, token};
use uniswap_v3_sdk::prelude::*;

mod strategy;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    chain_id: u64,
    rpc_url: String,

    base: ConfigToken,
    quote: ConfigToken,

    strategy: strategy::Config,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConfigToken {
    Native,

    Erc20 {
        symbol: String,
        address: String,
        decimals: u8,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ::env_logger::builder()
        .filter(None, log::LevelFilter::Info)
        .init();

    let config: Config = ::config::Config::builder()
        .add_source(config::File::with_name("./src/config.yaml"))
        .build()?
        .try_deserialize()?;

    let chain_id = config.chain_id;
    let rpc_url: Url = config.rpc_url.parse()?;

    let mut strategy = config.strategy.into_dyn();

    let provider = ProviderBuilder::new().on_http(rpc_url.clone());

    let base = to_token(&config.base, chain_id);
    let quote = to_token(&config.quote, chain_id);

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
                    CurrencyAmount::from_fractional_amount(
                        quote.clone(),
                        amount.numerator,
                        amount.denominator,
                    )?,
                    TradeType::ExactOutput,
                )?
            }
            strategy::Trade::Sell { amount } => {
                let route = Route::new(vec![pool], base.clone(), quote.clone());
                Trade::from_route(
                    route,
                    CurrencyAmount::from_fractional_amount(
                        base.clone(),
                        amount.numerator,
                        amount.denominator,
                    )?,
                    TradeType::ExactInput,
                )?
            }
        };

        let provider = ProviderBuilder::new().on_anvil_with_config(|anvil| {
            log::info!("Forking chain {chain_id} at {block}");
            anvil.fork(config.rpc_url.clone()).fork_block_number(block)
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

fn to_token(t: &ConfigToken, chain_id: u64) -> Currency {
    match t {
        ConfigToken::Native => Currency::NativeCurrency(Ether::on_chain(chain_id)),
        ConfigToken::Erc20 {
            symbol: name,
            address,
            decimals,
        } => Currency::Token(token!(chain_id, address, *decimals, name)),
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
