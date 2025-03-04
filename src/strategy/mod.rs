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
    Null {},
    AlwaysBuy(AlwaysBuy),
    Threshold(Threshold),
}

impl Config {
    pub fn into_dyn(self) -> Box<dyn Strategy> {
        match self {
            Config::Null {} => Box::new(Null),
            Config::AlwaysBuy(v) => Box::new(v),
            Config::Threshold(v) => Box::new(v),
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

pub struct Null;

impl Strategy for Null {
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
