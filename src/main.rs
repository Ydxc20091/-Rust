use std::env;
use std::error::Error;

use chrono::{TimeZone, Utc};
use reqwest::Url;
use serde::Deserialize;
use tabled::{settings::Style, Table, Tabled};

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Transaction {
    signature: String,
    timestamp: u64,
    native_transfers: Option<Vec<NativeTransfer>>,
    token_transfers: Option<Vec<TokenTransfer>>,
    events: Option<Events>,
    logs: Option<Vec<String>>,
    transaction_error: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct NativeTransfer {
    from_user_account: Option<String>,
    to_user_account: Option<String>,
    amount: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TokenTransfer {
    from_user_account: Option<String>,
    to_user_account: Option<String>,
    token_amount: Option<String>,
    mint: Option<String>,
    token_standard: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Events {
    swap: Option<Vec<SwapEvent>>,
    dex: Option<Vec<DexEvent>>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SwapEvent {
    source: Option<String>,
    liquidity_source: Option<String>,
    program_info: Option<ProgramInfo>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DexEvent {
    market: Option<String>,
    program_info: Option<ProgramInfo>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ProgramInfo {
    source: Option<String>,
    name: Option<String>,
    market: Option<String>,
}

#[derive(Tabled)]
struct Row {
    time: String,
    sig: String,
    exec: String,
    route: String,
    direction: String,
    sol_change: String,
    token_change: String,
    #[tabled(rename = "est_px_SOL")]
    est_px_sol: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let key = env::var("HELIUS_KEY").map_err(|_| "缺少 HELIUS_KEY 环境变量")?;
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        eprintln!("用法: {} <WALLET> [MINT] [LIMIT]", args[0]);
        return Ok(());
    }
    let owner = &args[1];
    let target_mint = args.get(2).cloned().unwrap_or_default();
    let limit: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(400);

    let client = reqwest::Client::new();
    let mut rows = Vec::new();
    let mut before: Option<String> = None;
    let mut got = 0;

    while got < limit {
        let take = std::cmp::min(100, limit - got);
        let page = fetch_page(&client, &key, owner, before.as_deref(), take).await?;
        if page.is_empty() { break; }
        for tx in &page {
            let (exec, route) = classify_exec(tx);
            let sol_net = net_native(tx, owner);
            let (tk_net, px_sol, token_change) = if !target_mint.is_empty() {
                let tk_net = net_token(tx, owner, &target_mint);
                let decimals = tx.token_transfers.as_ref()
                    .and_then(|v| v.iter().find(|t| t.mint.as_deref() == Some(&target_mint))
                        .and_then(|t| Some(if t.token_standard.as_deref() == Some("fungible") {6} else {6})))
                    .unwrap_or(6);
                let px_sol = price_from_flows(sol_net, tk_net, decimals);
                let token_change = (tk_net as f64) / 10f64.powi(decimals as i32);
                (tk_net, px_sol, format!("{}", token_change))
            } else {
                (0, None, String::new())
            };
            rows.push(Row {
                time: unix_to_iso(tx.timestamp),
                sig: format!("{}…", &tx.signature[..tx.signature.len().min(10)]),
                exec,
                route,
                direction: if target_mint.is_empty() { String::new() } else { dir_text(tk_net).to_string() },
                sol_change: format!("{:.9}", (sol_net as f64) / 1e9),
                token_change,
                est_px_sol: px_sol.map(|v| format!("{:.9}", v)).unwrap_or_default(),
            });
        }
        before = page.last().map(|tx| tx.signature.clone());
        got += page.len();
    }

    let rows: Vec<Row> = rows.into_iter().take(50).collect();
    let mut table = Table::new(rows);
    table.with(Style::modern());
    println!("{table}");

    Ok(())
}

async fn fetch_page(
    client: &reqwest::Client,
    key: &str,
    owner: &str,
    before: Option<&str>,
    take: usize,
) -> Result<Vec<Transaction>, Box<dyn Error>> {
    let mut url = Url::parse(&format!("https://api.helius.xyz/v0/addresses/{owner}/transactions"))?;
    url.query_pairs_mut()
        .append_pair("api-key", key)
        .append_pair("limit", &take.to_string());
    if let Some(b) = before {
        url.query_pairs_mut().append_pair("before", b);
    }
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(format!("Helius {}", resp.status()).into());
    }
    Ok(resp.json().await?)
}

fn net_native(tx: &Transaction, owner: &str) -> i128 {
    tx.native_transfers.as_ref().map_or(0, |list| {
        list.iter().fold(0i128, |acc, t| {
            let amt: i128 = t.amount.as_deref().unwrap_or("0").parse().unwrap_or(0);
            let mut acc = acc;
            if t.from_user_account.as_deref() == Some(owner) { acc -= amt; }
            if t.to_user_account.as_deref() == Some(owner) { acc += amt; }
            acc
        })
    })
}

fn net_token(tx: &Transaction, owner: &str, mint: &str) -> i128 {
    tx.token_transfers.as_ref().map_or(0, |list| {
        list.iter().filter(|t| t.mint.as_deref() == Some(mint)).fold(0i128, |acc, t| {
            let amt: i128 = t.token_amount.as_deref().unwrap_or("0").parse().unwrap_or(0);
            let mut acc = acc;
            if t.from_user_account.as_deref() == Some(owner) { acc -= amt; }
            if t.to_user_account.as_deref() == Some(owner) { acc += amt; }
            acc
        })
    })
}

fn classify_exec(tx: &Transaction) -> (String, String) {
    if let Some(ev) = &tx.events {
        if let Some(swaps) = &ev.swap {
            if !swaps.is_empty() {
                let route = swaps.iter().filter_map(|s|
                    s.source.clone()
                        .or(s.liquidity_source.clone())
                        .or(s.program_info.as_ref().and_then(|p| p.source.clone()))
                ).collect::<Vec<_>>().join(",");
                return ("SWAP".into(), if route.is_empty() {"router/amm".into()} else {route});
            }
        }
        if let Some(dexes) = &ev.dex {
            if !dexes.is_empty() {
                let name = dexes.iter().filter_map(|d|
                    d.market.clone()
                        .or(d.program_info.as_ref().and_then(|p| p.name.clone()))
                        .or(d.program_info.as_ref().and_then(|p| p.market.clone()))
                ).collect::<Vec<_>>().join(",").to_lowercase();
                if name.contains("phoenix") { return ("LIMIT".into(), "Phoenix".into()); }
                if name.contains("openbook") { return ("LIMIT".into(), "OpenBook".into()); }
                return ("LIMIT".into(), if name.is_empty() {"DEX".into()} else {name});
            }
        }
    }
    let logs = if tx.transaction_error.is_some() { vec![] } else { tx.logs.clone().unwrap_or_default() };
    let l = logs.join(" ").to_lowercase();
    if l.contains("placeorder") || l.contains("postonly") || l.contains("ioc") || l.contains("fok") {
        ("LIMIT".into(), "orderbook(?)".into())
    } else {
        ("OTHER".into(), String::new())
    }
}

fn price_from_flows(sol_lamports: i128, token_raw: i128, token_decimals: u32) -> Option<f64> {
    if sol_lamports == 0 || token_raw == 0 { return None; }
    let sol_abs = (sol_lamports.abs() as f64) / 1e9;
    let tok_abs = (token_raw.abs() as f64) / 10f64.powi(token_decimals as i32);
    if tok_abs == 0.0 { None } else { Some(sol_abs / tok_abs) }
}

fn dir_text(tk_net: i128) -> &'static str {
    if tk_net > 0 { "BUY" } else if tk_net < 0 { "SELL" } else { "NEUTRAL" }
}

fn unix_to_iso(ts: u64) -> String {
    Utc.timestamp_opt(ts as i64, 0).single().unwrap().format("%Y-%m-%dT%H:%M:%S").to_string()
}
