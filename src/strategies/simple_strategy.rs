use std::{
    collections::HashMap,
    env, fs,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Ok, Result};
use async_trait::async_trait;
use serde::Deserialize;

use bitfinex_api::{
    api::{
        authenticated::{
            funding::{
                active_funding_offers::{ActiveFundingOffers, ActiveFundingOffersResp},
                cancel_all_funding_offers::CancelAllFundingOffers,
                cancel_funding_offer::CancelFundingOffer,
                submit_funding_offer::SubmitFundingOffer,
                types::{FundingOffer, FundingOfferType},
            },
            wallets::{WalletResp, WalletType, Wallets, WalletsResp},
        },
        common::{Section, Sort, TimeFrame},
        ignore::ignore,
        public::candles::{AvailableCandles, Candles, HistCandlesResp},
        query::AsyncQuery,
    },
    bitfinex::AsyncBitfinex,
};

use super::Strategy;

pub struct SimpleStrategy {
    name: String,
    client: AsyncBitfinex,
    currency: String,
    min_amount: f64,
    max_balance_percent_per_loan: f64,
    min_rate: f64,
    target_period: u8,
    monitored_window: u64,
    nth_highest_candle: usize,
}

impl SimpleStrategy {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        client: AsyncBitfinex,
        currency: String,
        min_amount: f64,
        max_balance_percent_per_loan: f64,
        min_rate: f64,
        target_duration: u8,
        monitored_window: u64,
        nth_highest_candle: usize,
    ) -> Self {
        Self {
            name,
            client,
            currency,
            min_amount,
            max_balance_percent_per_loan,
            min_rate,
            target_period: target_duration,
            monitored_window,
            nth_highest_candle,
        }
    }

    /// Fetch the funding wallet from Bitfinex API.
    async fn funding_wallet(&self) -> Result<WalletResp> {
        let wallets: WalletsResp = Wallets::builder()
            .build()
            .unwrap()
            .query_async(&self.client)
            .await?;

        let funding_wallet = wallets
            .into_iter()
            .find(|wallet| wallet.ty == WalletType::Funding && wallet.currency == self.currency)
            .ok_or(anyhow!("Funding wallet not found"))?;

        Ok(funding_wallet)
    }

    /// Fetch the current active offer from Bitfinex API.
    async fn active_offer(&self) -> Result<Option<FundingOffer>> {
        let mut active_offers: ActiveFundingOffersResp = ActiveFundingOffers::builder()
            .symbol(&format!("f{}", self.currency))
            .build()
            .unwrap()
            .query_async(&self.client)
            .await?;

        // Prevent from having simulataneous active offers.
        if active_offers.len() > 1 {
            ignore(
                CancelAllFundingOffers::builder()
                    .currency(&self.currency)
                    .build()
                    .unwrap(),
            )
            .query_async(&self.client)
            .await?;

            bail!(
                "Detected {} active offers on {}, which have all been canceled",
                active_offers.len(),
                self.currency
            );
        }

        Ok(active_offers.pop())
    }

    /// Fetch the nth highest candles from the Bitfinex API.
    async fn get_highest_rate(&self, nth_highest_candle: usize, period: u8) -> Result<f64> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let start_mts = now - (self.monitored_window as u128 * 3600 * 1000);

        // Get the candles over le last 24 hours.
        let mut candles: HistCandlesResp = Candles::builder()
            .candles(AvailableCandles::FundingCandles {
                time_frame: TimeFrame::FiveMins,
                currency: &format!("f{}", self.currency),
                period,
            })
            .section(Section::Hist)
            .sort(Sort::Asc)
            .start(start_mts as _)
            .build()
            .unwrap()
            .query_async(&self.client)
            .await?;

        if candles.len() < nth_highest_candle {
            bail!("Not enough candles fetched");
        }

        candles.sort_by(|a, b| b.high.partial_cmp(&a.high).unwrap());

        Ok(candles[nth_highest_candle - 1].high)
    }

    /// Return the total and available balances (accounting for the current active offer, if any)
    fn compute_balances(
        &self,
        funding_wallet: &WalletResp,
        active_offer: &Option<FundingOffer>,
    ) -> (f64, f64) {
        let available_balance = funding_wallet.available_balance
            + active_offer
                .as_ref()
                .map_or(0., |active_offer| active_offer.amount);
        let total_balance = funding_wallet.balance;

        (available_balance, total_balance)
    }
}

#[async_trait]
impl Strategy for SimpleStrategy {
    type Output = Result<()>;

    fn from_config(path: &str) -> Vec<Self> {
        #[derive(Debug, Deserialize)]
        struct Strategy {
            keys: String,
            currency: String,
            min_amount: f64,
            max_balance_percent_per_loan: f64,
            min_rate: f64,
            target_period: u8,
            monitored_window: u64,
            nth_highest_candle: usize,
        }

        #[derive(Debug, Deserialize)]
        struct Config {
            simple_strategies: HashMap<String, Strategy>,
        }

        let config: Config = serde_yaml::from_str(&fs::read_to_string(path).unwrap()).unwrap();

        config
            .simple_strategies
            .into_iter()
            .map(|(name, strategy)| {
                let api_key_env = format!("API_KEY_{}", strategy.keys);
                let secret_key_env = format!("SECRET_KEY_{}", strategy.keys);

                Self::new(
                    name,
                    AsyncBitfinex::new_auth(
                        &env::var(&api_key_env)
                            .unwrap_or_else(|_| panic!("Missing {api_key_env} env variable")),
                        &env::var(&secret_key_env)
                            .unwrap_or_else(|_| panic!("Missing {secret_key_env} env variable")),
                    ),
                    strategy.currency,
                    strategy.min_amount,
                    strategy.max_balance_percent_per_loan,
                    strategy.min_rate,
                    strategy.target_period,
                    strategy.monitored_window,
                    strategy.nth_highest_candle,
                )
            })
            .collect()
    }

    /// Execute the strategy.
    async fn execute(&self) -> Self::Output {
        log::info!("Executing {} on {}...", self.name, self.currency);

        let funding_wallet = self.funding_wallet().await?;
        let active_offer = self.active_offer().await?;

        let (available_balance, total_balance) =
            self.compute_balances(&funding_wallet, &active_offer);

        // Early return if there is not enough available balance to create an offer.
        if available_balance < self.min_amount {
            log::info!(
                "Insufficient balance to submit a lend offer: {available_balance} < {}",
                self.min_amount
            );
            return Ok(());
        }

        // Query the nth highest rate.
        let mut period = self.target_period;
        let mut rate = self
            .get_highest_rate(self.nth_highest_candle, period)
            .await?;

        // If the rate is too low for the targeted duration, query for a period of 2 days.
        if rate < self.min_rate && period > 2 {
            period = 2;
            rate = self
                .get_highest_rate(self.nth_highest_candle, period)
                .await?;
        }

        // Take 99% of the highest rate.
        rate *= 0.99;

        // Clamp the amount to loan as a fraction of the total balance.
        let loan_amount = self
            .min_amount
            .max(available_balance.min(total_balance * self.max_balance_percent_per_loan));

        // Check if the active offer needs to be canceled.
        if let Some(active_offer) = active_offer {
            let rate_diff_percent = (active_offer.rate - rate).abs() / rate;
            let amount_diff = active_offer.amount - loan_amount;

            // Cancel the active offer if:
            //  - its rate is too far from the current one
            //  - or if its loan amount if different from the current one
            if rate_diff_percent > 0.01 || amount_diff > 1. {
                ignore(
                    CancelFundingOffer::builder()
                        .id(active_offer.id)
                        .build()
                        .unwrap(),
                )
                .query_async(&self.client)
                .await?;
            } else {
                log::info!(
                    "Active offer is good enough: {} @ {:.4}% per day ({:.2}% APR)",
                    active_offer.amount,
                    active_offer.rate * 100.,
                    active_offer.rate * 100. * 365.
                );
                return Ok(());
            }
        }

        ignore(
            SubmitFundingOffer::builder()
                .ty(FundingOfferType::Limit)
                .symbol(&format!("f{}", self.currency))
                .amount(loan_amount)
                .rate(rate)
                .period(period)
                .hidden(true)
                .build()
                .unwrap(),
        )
        .query_async(&self.client)
        .await?;

        log::info!(
            "Offer submitted: {} @ {:.4}% / day ({:.2}% APR)",
            loan_amount,
            rate * 100.,
            rate * 100. * 365.
        );

        Ok(())
    }
}
