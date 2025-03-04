# Lhava Take Home for Shelby Doolittle

Included is a solution to the "DeFi Trading Challenge".

- Implemented in Rust
- Using the `[uniswap_v3_sdk](https://crates.io/crates/uniswap_v3_sdk)` crate

## Running

Example:

```sh
cargo run -- src/config/always_sell_1_eth.yaml
```

Any config file:

```sh
cargo run -- <your config file>
# Config file is optional, default is src/config/default.yaml
```

See `src/config/default.yaml` for a documented example configuration.

## Design

I prioritized the simplicity of implementing trading strategies. I expect there to be many strategies, and therefore the implementation and integration of those should be kept as simple as possible.

## Possible Improvements

- Allow providing a private key and actually executing on chain
  - Currently, the system forks the chain with `anvil` and executes using a test account.
- Notifications for new blocks instead of polling
- Executing strategies in response to new transactions (before they show up in a block)
- More information in the TradeContext provided to a Strategy
  - Historical prices
  - Other asset prices
  - Liquidity of relevant assets
  - Trade history
- Multiple RPC urls, for reliability if a single provider goes down
- Protection against trades moving too much liquidity
