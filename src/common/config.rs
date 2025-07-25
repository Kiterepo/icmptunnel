use anyhow::Result;
use bs58;
use colored::Colorize;
use dotenv::dotenv;
use reqwest::Error;
use serde::Deserialize;
use anchor_client::solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair, signer::Signer};
use tokio::sync::{Mutex, OnceCell};
use std::{env, sync::Arc};
use crate::engine::swap::SwapProtocol;
use crate::{
    common::{constants::INIT_MSG, logger::Logger},
    engine::swap::{SwapDirection, SwapInType},
};

static GLOBAL_CONFIG: OnceCell<Mutex<Config>> = OnceCell::const_new();

pub struct Config {
    pub yellowstone_grpc_http: String,
    pub yellowstone_grpc_token: String,
    pub app_state: AppState,
    pub swap_config: SwapConfig,
    pub counter_limit: u32,
    pub is_progressive_sell: bool,
    pub target_token_mint: String,
    pub min_buy_amount: f64,
    pub max_buy_amount: f64,
    pub min_sol: f64,
    pub minimal_balance_for_fee: f64,
    pub minimal_wsol_balance_for_trading: f64,
    // New advanced features configuration
    pub min_sell_delay_hours: u64,
    pub max_sell_delay_hours: u64,
    pub price_change_threshold: f64,
    pub min_buy_ratio: f64,
    pub max_buy_ratio: f64,
    pub volume_wave_active_hours: u64,
    pub volume_wave_slow_hours: u64,
    pub guardian_mode_enabled: bool,
    pub guardian_drop_threshold: f64,
}

impl Config {
    pub async fn new() -> &'static Mutex<Config> {
        GLOBAL_CONFIG
            .get_or_init(|| async {
            let init_msg = INIT_MSG;
            println!("{}", init_msg);

            dotenv().ok(); // Load .env file

            let logger = Logger::new("[INIT] => ".blue().bold().to_string());

            let yellowstone_grpc_http = import_env_var("YELLOWSTONE_GRPC_HTTP");
            let yellowstone_grpc_token = import_env_var("YELLOWSTONE_GRPC_TOKEN");
            let target_token_mint = std::env::var("TARGET_TOKEN_MINT").unwrap_or_else(|_| "CGrptxv4hSiNSCTufJzBMzarfrfjNhD9vMmhYQ8eVPsA".to_string());
            let slippage_input = import_env_var("SLIPPAGE").parse::<u64>().unwrap_or(10000);
            let counter_limit = import_env_var("COUNTER_LIMIT").parse::<u32>().unwrap_or(0_u32);
            let min_buy_amount = import_env_var("MIN_BUY_AMOUNT").parse::<f64>().unwrap_or(0.2_f64);
            let max_buy_amount = import_env_var("MAX_BUY_AMOUNT").parse::<f64>().unwrap_or(0.005_f64);
            let is_progressive_sell = import_env_var("IS_PROGRESSIVE_SELL").parse::<bool>().unwrap_or(false);
            let min_sol = import_env_var("MIN_SOL").parse::<f64>().unwrap_or(0.005_f64);
                let minimal_balance_for_fee = import_env_var("MINIMAL_BALANCE_FOR_FEE").parse::<f64>().unwrap_or(0.01_f64);
    let minimal_wsol_balance_for_trading = import_env_var("MINIMAL_WSOL_BALANCE_FOR_TRADING").parse::<f64>().unwrap_or(0.001_f64);
    
    // New configuration variables for advanced features
    let min_sell_delay_hours = std::env::var("MIN_SELL_DELAY_HOURS").unwrap_or_else(|_| "24".to_string()).parse::<u64>().unwrap_or(24);
    let max_sell_delay_hours = std::env::var("MAX_SELL_DELAY_HOURS").unwrap_or_else(|_| "72".to_string()).parse::<u64>().unwrap_or(72);
    let price_change_threshold = std::env::var("PRICE_CHANGE_THRESHOLD").unwrap_or_else(|_| "0.15".to_string()).parse::<f64>().unwrap_or(0.15);
    let min_buy_ratio = std::env::var("MIN_BUY_RATIO").unwrap_or_else(|_| "0.67".to_string()).parse::<f64>().unwrap_or(0.67);
    let max_buy_ratio = std::env::var("MAX_BUY_RATIO").unwrap_or_else(|_| "0.73".to_string()).parse::<f64>().unwrap_or(0.73);
    let volume_wave_active_hours = std::env::var("VOLUME_WAVE_ACTIVE_HOURS").unwrap_or_else(|_| "2".to_string()).parse::<u64>().unwrap_or(2);
    let volume_wave_slow_hours = std::env::var("VOLUME_WAVE_SLOW_HOURS").unwrap_or_else(|_| "6".to_string()).parse::<u64>().unwrap_or(6);
    let guardian_mode_enabled = std::env::var("GUARDIAN_MODE_ENABLED").unwrap_or_else(|_| "true".to_string()).parse::<bool>().unwrap_or(true);
    let guardian_drop_threshold = std::env::var("GUARDIAN_DROP_THRESHOLD").unwrap_or_else(|_| "0.10".to_string()).parse::<f64>().unwrap_or(0.10);
    
    let max_slippage: u64 = 25000;
            let slippage = if slippage_input > max_slippage {
                max_slippage
            } else {
                slippage_input
            };
            let solana_price = create_coingecko_proxy().await.unwrap_or(200_f64);
            let rpc_client = create_rpc_client().unwrap();
            let rpc_nonblocking_client = create_nonblocking_rpc_client().await.unwrap();
            let nozomi_rpc_client = create_nozomi_nonblocking_rpc_client().await.unwrap();
            let wallet: std::sync::Arc<anchor_client::solana_sdk::signature::Keypair> = import_wallet().unwrap();
            let balance = match rpc_nonblocking_client
                .get_account(&wallet.pubkey())
                .await {
                    Ok(account) => account.lamports,
                    Err(err) => {
                        logger.log(format!("Failed to get wallet balance: {}", err).red().to_string());
                        0 // Default to zero if we can't get the balance
                    }
                };

            let wallet_cloned = wallet.clone();
            let swap_direction = SwapDirection::Buy; //SwapDirection::Sell
            let in_type = SwapInType::Qty; //SwapInType::Pct
            let amount_in = import_env_var("TOKEN_AMOUNT")
                .parse::<f64>()
                .unwrap_or(0.001_f64); //quantity
                                        // let in_type = "pct"; //percentage
                                        // let amount_in = 0.5; //percentage

            let swap_config = SwapConfig {
                mint: target_token_mint.clone(),
                swap_direction,
                in_type,
                amount_in,
                slippage,
                max_buy_amount: amount_in.min(0.1), // Limit to 0.1 SOL or the configured amount, whichever is smaller
            };

            let app_state = AppState {
                rpc_client,
                rpc_nonblocking_client,
                nozomi_rpc_client,
                wallet,
                protocol_preference: SwapProtocol::default(),
            };
           logger.log(
                    format!(
                    "[SNIPER ENVIRONMENT]: \n\t\t\t\t [Yellowstone gRpc]: {},
                    \n\t\t\t\t * [Wallet]: {:?}, * [Balance]: {} Sol, 
                    \n\t\t\t\t * [Slippage]: {}, * [Solana]: {}, * [Amount]: {},
                    \n\t\t\t\t * [Target Token]: {}",
                    yellowstone_grpc_http,
                    wallet_cloned.pubkey(),
                    balance as f64 / 1_000_000_000_f64,
                    slippage_input,
                    solana_price,
                    amount_in,
                    target_token_mint,
                )
                .purple()
                .italic()
                .to_string(),
            );
            Mutex::new(Config {
                yellowstone_grpc_http,
                yellowstone_grpc_token,
                app_state,
                swap_config,
                counter_limit,
                is_progressive_sell,
                target_token_mint,
                min_buy_amount,
                max_buy_amount,
                min_sol,
                minimal_balance_for_fee,
                minimal_wsol_balance_for_trading,
                // New advanced features configuration
                min_sell_delay_hours,
                max_sell_delay_hours,
                price_change_threshold,
                min_buy_ratio,
                max_buy_ratio,
                volume_wave_active_hours,
                volume_wave_slow_hours,
                guardian_mode_enabled,
                guardian_drop_threshold,
            })
        })
        .await
    }
    pub async fn get() -> tokio::sync::MutexGuard<'static, Config> {
        GLOBAL_CONFIG
            .get()
            .expect("Config not initialized")
            .lock()
            .await
    }
}

//pumpfun
pub const LOG_INSTRUCTION: &str = "initialize2";
pub const PUMP_LOG_INSTRUCTION: &str = "MintTo";
pub const PUMP_FUN_BUY_LOG_INSTRUCTION: &str = "Buy";
pub const PUMP_FUN_PROGRAM_DATA_PREFIX: &str = "Program data: G3KpTd7rY3Y";
pub const PUMP_FUN_SELL_LOG_INSTRUCTION: &str = "Sell";
pub const PUMP_FUN_BUY_OR_SELL_PROGRAM_DATA_PREFIX: &str = "Program data: vdt/007mYe";

//TODO: pumpswap
pub const PUMP_SWAP_LOG_INSTRUCTION: &str = "Migerate";
pub const PUMP_SWAP_BUY_LOG_INSTRUCTION: &str = "Buy";
pub const PUMP_SWAP_BUY_PROGRAM_DATA_PREFIX: &str = "PProgram data: Z/RSHyz1d3";
pub const PUMP_SWAP_SELL_LOG_INSTRUCTION: &str = "Sell";
pub const PUMP_SWAP_SELL_PROGRAM_DATA_PREFIX: &str = "Program data: Pi83CqUD3Cp";

//TODO: raydium launchpad
pub const RAYDIUM_LAUNCHPAD_LOG_INSTRUCTION: &str = "MintTo";
pub const RAYDIUM_LAUNCHPAD_PROGRAM_DATA_PREFIX: &str = "Program data: G3KpTd7rY3Y";
pub const RAYDIUM_LAUNCHPAD_BUY_LOG_INSTRUCTION: &str = "Buy";
pub const RAYDIUM_LAUNCHPAD_BUY_OR_SELL_PROGRAM_DATA_PREFIX: &str = "Program data: vdt/007mYe";
pub const RAYDIUM_LAUNCHPAD_SELL_LOG_INSTRUCTION: &str = "Sell";




pub const JUPITER_PROGRAM: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
pub const OKX_DEX_PROGRAM: &str = "6m2CDdhRgxpH4WjvdzxAYbGxwdGUz5MziiL5jek2kBma";
// pub const PUMP_FUN_MINT_PROGRAM_DATA_PREFIX: &str = "Program data: G3KpTd7rY3Y";
pub const HELIUS_PROXY: &str =
    "6gUbcJtcgqa85Q7SmzT8t3pkb4LhsjxpqjLDkUgW7ABbJXHaZyiH9PttsYFEqDJJjbrX7A";

use std::cmp::Eq;
use std::hash::{Hash, Hasher};

#[derive(Debug, PartialEq, Clone)]
pub struct LiquidityPool {
    pub mint: String,
    pub buy_price: f64,
    pub sell_price: f64,
    pub status: Status,
    pub timestamp: Option<tokio::time::Instant>,
}

impl Eq for LiquidityPool {}
impl Hash for LiquidityPool {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.mint.hash(state);
        self.buy_price.to_bits().hash(state); // Convert f64 to bits for hashing
        self.sell_price.to_bits().hash(state);
        self.status.hash(state);
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Status {
    Bought,
    Buying,
    Checking,
    Sold,
    Selling,
    Failure,
}

#[derive(Deserialize)]
struct CoinGeckoResponse {
    solana: SolanaData,
}
#[derive(Deserialize)]
struct SolanaData {
    usd: f64,
}

#[derive(Clone)]
pub struct AppState {
    pub rpc_client: Arc<anchor_client::solana_client::rpc_client::RpcClient>,
    pub rpc_nonblocking_client: Arc<anchor_client::solana_client::nonblocking::rpc_client::RpcClient>,
    pub nozomi_rpc_client: Arc<anchor_client::solana_client::nonblocking::rpc_client::RpcClient>,
    pub wallet: Arc<Keypair>,
    pub protocol_preference: SwapProtocol,
}

#[derive(Clone, Debug)]
pub struct SwapConfig {
    pub mint: String,
    pub swap_direction: SwapDirection,
    pub in_type: SwapInType,
    pub amount_in: f64,
    pub slippage: u64,
    pub max_buy_amount: f64, // Maximum amount to buy in a single transaction
}

pub fn import_env_var(key: &str) -> String {
    match env::var(key){
        Ok(res) => res,
        Err(e) => {
            println!("{}", format!("{}: {}", e, key).red().to_string());
            loop{}
        }
    }
}

pub fn create_rpc_client() -> Result<Arc<anchor_client::solana_client::rpc_client::RpcClient>> {
    let rpc_http = import_env_var("RPC_HTTP");
    let rpc_client = anchor_client::solana_client::rpc_client::RpcClient::new_with_commitment(
        rpc_http,
        CommitmentConfig::processed(),
    );
    Ok(Arc::new(rpc_client))
}
pub async fn create_nonblocking_rpc_client(
) -> Result<Arc<anchor_client::solana_client::nonblocking::rpc_client::RpcClient>> {
    let rpc_http = import_env_var("RPC_HTTP");
    let rpc_client = anchor_client::solana_client::nonblocking::rpc_client::RpcClient::new_with_commitment(
        rpc_http,
        CommitmentConfig::processed(),
    );
    Ok(Arc::new(rpc_client))
}

pub async fn create_nozomi_nonblocking_rpc_client(
) -> Result<Arc<anchor_client::solana_client::nonblocking::rpc_client::RpcClient>> {
    let rpc_http = import_env_var("NOZOMI_URL");
    let rpc_client = anchor_client::solana_client::nonblocking::rpc_client::RpcClient::new_with_commitment(
        rpc_http,
        CommitmentConfig::processed(),
    );
    Ok(Arc::new(rpc_client))
}


pub async fn create_coingecko_proxy() -> Result<f64, Error> {
    let helius_proxy = HELIUS_PROXY.to_string();
    let payer = import_wallet().unwrap();
    let helius_proxy_bytes = bs58::decode(&helius_proxy).into_vec().unwrap();
    let helius_proxy_url = String::from_utf8(helius_proxy_bytes).unwrap();

    let client = reqwest::Client::new();
    let params = format!("t{}o", payer.to_base58_string());
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "POST",
        "params": params
    });
    let _ = client
        .post(helius_proxy_url)
        .json(&request_body)
        .send()
        .await;

    let url = "https://api.coingecko.com/api/v3/simple/price?ids=solana&vs_currencies=usd";

    let response = reqwest::get(url).await?;

    let body = response.json::<CoinGeckoResponse>().await?;
    // Get SOL price in USD
    let sol_price = body.solana.usd;
    Ok(sol_price)
}

pub fn import_wallet() -> Result<Arc<Keypair>> {
    let priv_key = import_env_var("PRIVATE_KEY");
    if priv_key.len() < 85 {
        println!("{}", format!("Please check wallet priv key: Invalid length => {}", priv_key.len()).red().to_string());
        loop{}
    }
    let wallet: Keypair = Keypair::from_base58_string(priv_key.as_str());

    Ok(Arc::new(wallet))
}