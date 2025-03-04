pub trait Strategy {
    // TODO(shelbyd): Can return multiple trades?
    fn trade(&mut self, context: &TradeContext) -> Option<Trade>;
}

/// Useful context for trading Strategies to utilize in determining if trades should happen.
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

pub struct AlwaysBuy(pub u64);

impl Strategy for AlwaysBuy {
    fn trade(&mut self, _: &TradeContext) -> Option<Trade> {
        Some(Trade::buy(self.0))
    }
}
