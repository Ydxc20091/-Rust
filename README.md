# Helius 交易解码器（Rust 版）

一个用 Rust 编写的命令行工具，调用 [Helius](https://helius.xyz) API 拉取指定钱包的交易记录，并按 SWAP/LIMIT 等类型进行分类统计。逻辑参考自常见的 JavaScript 脚本，并封装成完整项目。

## 功能概览

- 分页抓取指定地址的交易记录
- 统计 SOL 与指定代币的净流入/净流出
- 根据 Helius 解析的 events/日志判断执行类型（SWAP、LIMIT、OTHER）
- 简单估算成交价格并以表格方式输出

## 环境准备

1. **安装 Rust**
   ```bash
   curl https://sh.rustup.rs -sSf | sh
   source "$HOME/.cargo/env"
   rustc -V        # 确认安装成功
   ```
2. **获取 Helius API Key**
   在 [Helius 官网](https://helius.xyz) 创建账户并生成 API Key，将其写入环境变量：
   ```bash
   export HELIUS_KEY=你的Helius密钥
   ```

## 编译与运行

1. **编译**
   ```bash
   cargo build
   ```
2. **执行**
   ```bash
   HELIUS_KEY=你的Helius密钥 cargo run -- <钱包地址> [代币Mint] [抓取上限]
   # 示例：
   HELIUS_KEY=xxx cargo run -- 7xxxxx... 3AvXA85w... 800
   ```

参数说明：
- `<钱包地址>`：必填，被查询的钱包。
- `[代币Mint]`：可选，只统计该代币的净变化。
- `[抓取上限]`：可选，最多抓取的交易笔数（默认 400，最大每次分页 100）。

## 输出字段说明

| 字段 | 含义 |
| --- | --- |
| `time` | 交易时间（UTC） |
| `sig` | 交易签名前 10 位 |
| `exec` / `route` | 执行类型及路由（SWAP、LIMIT、Phoenix/OpenBook 等） |
| `direction` | 对目标代币而言是 BUY、SELL 还是 NEUTRAL |
| `sol_change` | 本次交易净变动的 SOL 数量 |
| `token_change` | 本次交易净变动的目标代币数量 |
| `est_px_SOL` | 估算的每个代币折算成 SOL 的价格 |

只保留最近 50 条交易的简表输出，进一步分析可在此基础上自行扩展。

## 注意事项

- 程序依赖网络请求，请确保能访问 `api.helius.xyz`。
- Helius 的免费额度有限，频繁调用请留意速率限制。
- 本项目仅示例学习之用，使用前请自行评估风险。

