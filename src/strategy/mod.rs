use serde::Deserialize;
use uniswap_sdk_core::prelude::Fraction;

pub trait Strategy {
    // TODO(shelbyd): Can return multiple trades?
    fn trade(&mut self, ctx: &TradeContext) -> Option<Trade>;
}

/// Useful context for trading Strategies to utilize in determining if trades should happen.
// TODO(shelbyd): Uniswap's quoting available here.
#[derive(Debug)]
pub struct TradeContext {
    pub price_lossy: f64,
}

// TODO(shelbyd): Restrictions on execution, like max-rate. Basically things that go in UniSwap SwapOptions.
#[derive(Debug)]
pub enum Trade {
    Buy { amount: Fraction },
    Sell { amount: Fraction },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Config {
    Empty(()),
    AlwaysBuy(AlwaysBuy),
    AlwaysSell(AlwaysSell),
    Threshold(Threshold),
    Ema { carry: f64, inner: Box<Config> },
}

impl Config {
    pub fn into_dyn(self) -> Box<dyn Strategy> {
        match self {
            Config::Empty(()) => Box::new(Empty),
            Config::AlwaysBuy(v) => Box::new(v),
            Config::AlwaysSell(v) => Box::new(v),
            Config::Threshold(v) => Box::new(v),
            Config::Ema { carry, inner } => {
                let inner = inner.into_dyn();
                Box::new(ExponentialMovingAverage {
                    carry,
                    inner,
                    last: None,
                })
            }
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(untagged)]
// TODO(shelbyd): Allow decimal input.
enum FractionInput {
    Int(u64),
    Fraction { numerator: u64, denominator: u64 },
}

impl Into<Fraction> for FractionInput {
    fn into(self) -> Fraction {
        match self {
            FractionInput::Int(i) => Fraction::new(i, 1),
            FractionInput::Fraction {
                numerator,
                denominator,
            } => Fraction::new(numerator, denominator),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct AlwaysBuy(FractionInput);

impl Strategy for AlwaysBuy {
    fn trade(&mut self, _: &TradeContext) -> Option<Trade> {
        Some(Trade::Buy {
            amount: self.0.into(),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct AlwaysSell(FractionInput);

impl Strategy for AlwaysSell {
    fn trade(&mut self, _: &TradeContext) -> Option<Trade> {
        Some(Trade::Sell {
            amount: self.0.into(),
        })
    }
}

pub struct Empty;

impl Strategy for Empty {
    fn trade(&mut self, _: &TradeContext) -> Option<Trade> {
        None
    }
}

#[derive(Debug, Deserialize)]
pub struct Threshold {
    buy: Option<ThresholdPoint>,
    sell: Option<ThresholdPoint>,
}

#[derive(Debug, Deserialize)]
struct ThresholdPoint {
    at: f64,
    amount: FractionInput,
}

impl Strategy for Threshold {
    fn trade(&mut self, ctx: &TradeContext) -> Option<Trade> {
        if let Some(buy) = &self.buy {
            if ctx.price_lossy <= buy.at {
                return Some(Trade::Buy {
                    amount: buy.amount.into(),
                });
            }
        }

        if let Some(sell) = &self.sell {
            if ctx.price_lossy >= sell.at {
                return Some(Trade::Sell {
                    amount: sell.amount.into(),
                });
            }
        }

        None
    }
}

/// Composable wrapper strategy that provides an exponential moving average price to the inner strategy.
pub struct ExponentialMovingAverage {
    // TODO(shelbyd): Make possible to provide as type parameter.
    inner: Box<dyn Strategy>,

    /// The fraction [0, 1] to multiply the previous price by. Usually ~0.9.
    carry: f64,

    last: Option<f64>,
}

impl Strategy for ExponentialMovingAverage {
    fn trade(&mut self, ctx: &TradeContext) -> Option<Trade> {
        let price = self
            .last
            .map(|p| p * self.carry + ctx.price_lossy * (1. - self.carry))
            .unwrap_or(ctx.price_lossy);
        self.last = Some(price);

        log::info!("Giving inner strategy price as {price}");

        self.inner.trade(&TradeContext { price_lossy: price })
    }
}
