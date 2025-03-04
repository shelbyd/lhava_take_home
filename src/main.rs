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

        let provider = ProviderBuilder::new().on_anvil_with_config(|anvil| {
            log::info!("Forking chain {chain_id} at {block}");
            anvil.fork(config.rpc_url.clone()).fork_block_number(block)
        });
        let account = provider.get_accounts().await?[0];

        let params = match trade {
            strategy::Trade::Buy { amount } => {
                let route = Route::new(vec![pool], base.clone(), quote.clone());
                let trade = Trade::from_route(
                    route,
                    from_human_amount(amount, &quote)?,
                    TradeType::ExactOutput,
                )?;
                swap_call_parameters(
                    &mut [trade],
                    SwapOptions {
                        recipient: account,
                        ..Default::default()
                    },
                )?
            }
            strategy::Trade::Sell { amount } => {
                let route = Route::new(vec![pool], quote.clone(), base.clone());
                let trade = Trade::from_route(
                    route,
                    from_human_amount(amount, &quote)?,
                    TradeType::ExactInput,
                )?;
                swap_call_parameters(
                    &mut [trade],
                    SwapOptions {
                        recipient: account,
                        ..Default::default()
                    },
                )?
            }
        };

        let tx = TransactionRequest::default()
            .from(account)
            .to(*SWAP_ROUTER_02_ADDRESSES
                .get(&chain_id)
                .context(format!("Unknown swap router for chain id {chain_id}"))?)
            .input(params.calldata.into())
            .value(params.value);

        log_balance("(base) before trade", account, &base, &provider).await?;
        log_balance("(quot) before trade", account, &quote, &provider).await?;

        let hash = provider.send_transaction(tx).await?.watch().await?;
        log::info!("Successfully executed transaction {hash}");

        log_balance("(base) after trade", account, &base, &provider).await?;
        log_balance("(quot) after trade", account, &quote, &provider).await?;

        return Ok(());
    }
}

async fn log_balance(
    suffix: &str,
    account: Address,
    currency: &Currency,
    provider: &impl alloy::providers::Provider,
) -> anyhow::Result<()> {
    alloy::sol! {
        #[sol(rpc)]
        interface ERC20 {
            function balanceOf(address target) returns (uint256);
        }
    }

    let balance = match currency {
        Currency::NativeCurrency(_) => provider.get_balance(account).await?,
        Currency::Token(t) => {
            let erc20 = ERC20::new(t.address(), provider);
            erc20.balanceOf(account).call().await?._0
        }
    };

    log::info!(
        "{account} has {} {} {suffix}",
        CurrencyAmount::from_raw_amount(currency, balance.to_big_int())?.to_exact(),
        currency.symbol().map_or("???", |v| v)
    );

    Ok(())
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

fn from_human_amount(
    amount: Fraction,
    currency: &Currency,
) -> anyhow::Result<CurrencyAmount<Currency>> {
    let amount = CurrencyAmount::from_fractional_amount(
        currency.clone(),
        amount.numerator,
        amount.denominator,
    )?;
    Ok(amount.multiply(&Fraction::new(amount.meta().decimal_scale.clone(), 1))?)
}
