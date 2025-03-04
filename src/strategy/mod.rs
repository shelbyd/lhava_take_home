pub trait Strategy {
    // TODO(shelbyd): Can return multiple trades?
    fn trade(&mut self, ctx: &TradeContext) -> Option<Trade>;
}

/// Useful context for trading Strategies to utilize in determining if trades should happen.
#[derive(Debug)]
pub struct TradeContext {
    pub price_lossy: f64,
}

#[derive(Debug)]
pub enum Trade {
    Buy { amount: u64 },
}

impl Trade {
    fn buy(amount: u64) -> Trade {
        Trade::Buy { amount }
    }
}

#[allow(unused)]
pub struct AlwaysBuy(pub u64);

impl Strategy for AlwaysBuy {
    fn trade(&mut self, _: &TradeContext) -> Option<Trade> {
        Some(Trade::buy(self.0))
    }
}

#[allow(unused)]
pub struct Null;

impl Strategy for Null {
    fn trade(&mut self, _: &TradeContext) -> Option<Trade> {
        None
    }
}
