# Sarkhati - Automated Order Sender

A Rust application that automatically sends trading orders to Iranian stock brokers.

## Supported Brokers

- **MofidOnlinePlus** (mofid_online_plus) - https://tg.mofidonline.com
- **Danayan** (danayan) - https://trader.danayan.broker
- **Online Plus Brokers** (online_plus) - configurable via `config_online_plus.json`
- **Easy Trader** (easy_trader) - configurable via `config_easy_trader.json`
- **Exir Brokers** (exir) - configurable via `config_exir.json`
- **Bidar Trader** (bidar) - https://bidartrader.ir

## Features

- Multi-broker support
- Cookie or Bearer token authentication
- Continuous order sending in a loop
- Multiple orders in parallel
- Configurable batch delay
- Separate config files per broker

## Prerequisites

- Rust (latest stable version)
- Valid broker account
- Browser cookies or authorization token from an active session

---

## Quick Start

### 1. Copy the example config for your broker:

```bash
# For MofidOnlinePlus
cp config_mofid_online_plus.example.json config_mofid_online_plus.json

# For Danayan
cp config_danayan.example.json config_danayan.json

# For Online Plus brokers
cp config_online_plus.example.json config_online_plus.json

# For Easy Trader
cp config_easy_trader.example.json config_easy_trader.json

# For Exir brokers
cp config_exir.example.json config_exir.json

# For Bidar Trader
cp config_bidar.example.json config_bidar.json

# Optional global config (NTP time reference)
cp config_global.example.json config_global.json
```

### 2. Get your authentication credentials

See [Authentication Guide](#authentication-guide) below.

### 3. Build and run:

```bash
cargo build --release

# For MofidOnlinePlus
cargo run --release -- mofid_online_plus

# For Danayan
cargo run --release -- danayan

# For Online Plus brokers
cargo run --release -- online_plus

# For Easy Trader
cargo run --release -- easy_trader

# For Exir brokers
cargo run --release -- exir

# For Bidar Trader
cargo run --release -- bidar

# For ALL brokers in parallel
cargo run --release -- all
```

Press `Ctrl+C` to stop.

---

## Configuration

### Global Settings (`config_global.json`)

Use this optional file to define shared settings, including the NTP server used to correct the
system clock reference for scheduled orders.

```json
{
  "ntp": {
    "server": "pool.ntp.org",
    "timeout_ms": 1000
  }
}
```

### MofidOnlinePlus (`config_mofid_online_plus.json`)

You can provide a single account object  or an `accounts` array for multiple MofidOnlinePlus accounts.

```json
{
  "accounts": [
    {
      "name": "primary",
      "cookie": "",
      "authorization": "YOUR_BEARER_TOKEN",
      "user_agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36",
      "order_url": "https://mofidonline.com/apigateway/api/v1/Order/send",
      "batch_delay_ms": 100,
      "orders": [
        {
          "orderSide": "Buy",
          "price": 2474,
          "quantity": 1,
          "symbolIsin": "IRO1NMAD0001",
          "validityType": 0,
          "validityDate": null,
          "orderFrom": "Titan"
        }
      ]
    }
  ]
}
```

#### MofidOnlinePlus Order Parameters

| Field | Description |
|-------|-------------|
| `orderSide` | `"Buy"` or `"Sell"` |
| `price` | Order price |
| `quantity` | Number of shares |
| `symbolIsin` | Stock ISIN code (e.g., `"IRO1NMAD0001"`) |
| `validityType` | `0` for day order |
| `validityDate` | `null` for day orders |
| `orderFrom` | Platform identifier (`"Titan"`) |

### Online Plus Brokers (`config_online_plus.json`)

You can provide a single account object  or an `accounts` array for multiple Online Plus brokers.

```json
{
  "accounts": [
    {
      "name": "broker_name",
      "cookie": "YOUR_COOKIE_HERE",
      "user_agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36",
      "order_url": "https://example-broker.ir/Web/V1/Order/Post",
      "origin": "https://example-broker.ir",
      "referer": "https://example-broker.ir/",
      "batch_delay_ms": 100,
      "orders": [
        {
          "IsSymbolCautionAgreement": false,
          "CautionAgreementSelected": false,
          "IsSymbolSepahAgreement": false,
          "SepahAgreementSelected": false,
          "orderCount": 1,
          "orderPrice": 2474,
          "FinancialProviderId": 1,
          "minimumQuantity": 0,
          "maxShow": 0,
          "orderId": 0,
          "isin": "IRO1NMAD0001",
          "orderSide": 65,
          "orderValidity": 74,
          "orderValiditydate": null,
          "shortSellIsEnabled": false,
          "shortSellIncentivePercent": 0
        }
      ]
    }
  ]
}
```

#### Online Plus Order Parameters

| Field | Description |
|-------|-------------|
| `orderSide` | `65` for Buy, `86` for Sell |
| `orderPrice` | Order price |
| `orderCount` | Number of shares |
| `isin` | Stock ISIN code |
| `orderValidity` | `74` for day order |
| `orderValiditydate` | `null` for day orders |
| `FinancialProviderId` | Usually `1` |
| `minimumQuantity` | Minimum fill quantity (`0` for any) |
| `maxShow` | Max visible quantity (`0` for all) |

### Easy Trader (`config_easy_trader.json`)

You can provide a single account object  or an `accounts` array for multiple Easy Trader accounts.

```json
{
  "accounts": [
    {
      "name": "easy_trader",
      "authorization": "YOUR_BEARER_TOKEN",
      "user_agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36",
      "order_url": "https://api-mts.orbis.easytrader.ir/core/api/v2/order",
      "batch_delay_ms": 100,
      "orders": [
        {
          "price": 3432,
          "quantity": 400000,
          "side": 0,
          "validityType": 0,
          "symbolIsin": "IRO1SPHR0001",
          "orderModelType": 1,
          "orderFrom": 34
        }
      ]
    }
  ]
}
```

#### Easy Trader Order Parameters

| Field | Description |
|-------|-------------|
| `side` | `0` for Buy, `1` for Sell |
| `price` | Order price |
| `quantity` | Number of shares |
| `symbolIsin` | Stock ISIN code |
| `validityType` | Usually `0` for day order |
| `orderModelType` | Usually `1` for limit order |
| `orderFrom` | Platform identifier (example: `34`) |

Buy payload sent by Sarkhati:

```json
{
  "order": {
    "price": 3432,
    "quantity": 400000,
    "side": 0,
    "validityType": 0,
    "symbolIsin": "IRO1SPHR0001",
    "orderModelType": 1,
    "orderFrom": 34
  }
}
```

Sell payload example (only `side` changes to `1`):

```json
{
  "order": {
    "price": 3432,
    "quantity": 400000,
    "side": 1,
    "validityType": 0,
    "symbolIsin": "IRO1SPHR0001",
    "orderModelType": 1,
    "orderFrom": 34
  }
}
```

### Exir Brokers (`config_exir.json`)

```json
{
  "accounts": [
    {
      "name": "broker_name",
      "cookie": "YOUR_COOKIE_HERE",
      "nt": "YOUR_NT_TOKEN",
      "user_agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36",
      "order_url": "https://example.exirbroker.com/api/v1/order",
      "origin": "https://example.exirbroker.com",
      "referer": "https://example.exirbroker.com/exir/mainNew",
      "batch_delay_ms": 100,
      "orders": [
        {
          "insMaxLcode": "IRO1NMAD0001",
          "bankAccountId": -1,
          "side": "SIDE_BUY",
          "orderType": "ORDER_TYPE_LIMIT",
          "quantity": 1,
          "price": 2474,
          "validityType": "VALIDITY_TYPE_DAY",
          "validityDate": "",
          "coreType": "c",
          "hasUnderCautionAgreement": false,
          "dividedOrder": false
        }
      ]
    }
  ]
}
```

#### Exir Order Parameters

| Field | Description |
|-------|-------------|
| `insMaxLcode` | Stock ISIN code |
| `bankAccountId` | Bank account id (`-1` for default) |
| `side` | `SIDE_BUY` or `SIDE_SELL` |
| `orderType` | `ORDER_TYPE_LIMIT` for limit order |
| `quantity` | Number of shares |
| `price` | Order price |
| `validityType` | `VALIDITY_TYPE_DAY` for day order |
| `validityDate` | Empty string for day orders |
| `coreType` | Core type (`"c"`) |
| `hasUnderCautionAgreement` | `true`/`false` |
| `dividedOrder` | `true`/`false` |

### Danayan (`config_danayan.json`)

You can provide a single account object  or an `accounts` array for multiple Danayan accounts.

```json
{
  "accounts": [
    {
      "name": "primary",
      "cookie": "YOUR_COOKIE_HERE",
      "user_agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36",
      "order_url": "https://otapi.danayan.broker/api/v1/TseOms/RegisterOrder",
      "batch_delay_ms": 100,
      "orders": [
        {
          "orderValidityType": 1,
          "orderPaymentGateway": 1,
          "price": 2474,
          "quantity": 1,
          "disclosedQuantity": null,
          "isin": "IRO1NMAD0001",
          "orderSide": 1
        }
      ]
    }
  ]
}
```

#### Danayan Order Parameters

| Field | Description |
|-------|-------------|
| `orderSide` | `1` for Buy, `2` for Sell |
| `price` | Order price |
| `quantity` | Number of shares |
| `isin` | Stock ISIN code |
| `orderValidityType` | `1` for day order |
| `orderPaymentGateway` | Usually `1` |
| `disclosedQuantity` | Disclosed quantity (`null` for all) |

### Bidar Trader (`config_bidar.json`)

You can provide a single account object  or an `accounts` array for multiple Bidar accounts.

```json
{
  "accounts": [
    {
      "name": "primary",
      "authorization": "YOUR_BEARER_TOKEN_HERE",
      "user_agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36",
      "order_url": "https://api.bidartrader.ir/trader/v1/order/buy",
      "x_user_trace": "MjQ4MDMwMzYwODpJUk8xTk1BRDAwMDE=",
      "batch_delay_ms": 100,
      "orders": [
        {
          "type": "LIMIT",
          "quantity": "1",
          "isin": "IRO1NMAD0001",
          "validity": "DAY",
          "price": "2548"
        }
      ]
    }
  ]
}
```

#### Bidar Trader Order Parameters

| Field | Description |
|-------|-------------|
| `type` | `"LIMIT"` for limit order |
| `price` | Order price (as string) |
| `quantity` | Number of shares (as string) |
| `isin` | Stock ISIN code |
| `validity` | `"DAY"` for day order |

---

## Authentication Guide

### MofidOnlinePlus

MofidOnlinePlus supports both **Cookie** and **Bearer token** authentication.

#### Option A: Bearer Token (Recommended)

1. Open Chrome and go to https://tg.mofidonline.com/
2. Log in with your credentials
3. Press `F12` to open Developer Tools
4. Click the **Network** tab
5. Look for requests to `mofidonline.com/apigateway`
6. Find the `Authorization` header in Request Headers
7. Copy everything after `Bearer ` (just the token)
8. Paste in `config_mofid_online_plus.json` → `authorization` field

#### Option B: Cookie

1. Open Chrome and go to https://tg.mofidonline.com/
2. Log in with your credentials
3. Press `F12` → **Network** tab → Refresh page
4. Click on a request to `tg.mofidonline.com`
5. Find `Cookie:` in Request Headers
6. Copy the entire cookie string
7. Paste in `config_mofid_online_plus.json` → `cookie` field

### Danayan

Danayan uses **Cookie** authentication (contains embedded Authorization token).

1. Open Chrome and go to https://trader.danayan.broker/
2. Log in with your credentials
3. Press `F12` → **Network** tab
4. Look for requests to `otapi.danayan.broker`
5. Find `Cookie:` in Request Headers
6. Copy the entire cookie string (includes `Authorization=Bearer%20...`)
7. Paste in `config_danayan.json` → `cookie` field

### Online Plus Brokers

Online Plus brokers use **Cookie** authentication only.

1. Open your broker's trading web app
2. Log in with your credentials
3. Press `F12` → **Network** tab
4. Find the order API request and copy the `Cookie:` header
5. Paste in `config_online_plus.json` → `cookie` field

### Easy Trader

Easy Trader uses **Bearer token** authentication.

1. Open Easy Trader web app and log in
2. Press `F12` → **Network** tab
3. Find requests to `api-mts.orbis.easytrader.ir`
4. Copy `Authorization: Bearer ...` token value
5. Paste in `config_easy_trader.json` → `authorization` field

### Exir Brokers

Exir brokers use **Cookie** authentication and require an **`nt` token** for generating the dynamic `X-App-N` header.

#### Getting the Cookie

1. Open your Exir broker web app
2. Log in with your credentials
3. Press `F12` → **Network** tab
4. Find the order API request and copy the `Cookie:` header
5. Paste in `config_exir.json` → `cookie` field

#### Getting the `nt` Token

The `nt` token is required to calculate the `X-App-N` header dynamically. To find it:

1. Log in to your Exir broker web app
2. Press `F12` → **Console** tab
3. Paste and run this code:

```javascript
// Get the session from localStorage
const session = JSON.parse(localStorage.getItem("session"));

// Access userInfo
const userInfo = session.userInfo;

// Display the nt value
console.log("nt value:", userInfo.nt);

// Display the full userInfo object
console.log("Full userInfo:", userInfo);
```

4. Copy the `nt` value from the console output
5. Paste in `config_exir.json` → `nt` field

### Bidar Trader

Bidar Trader uses **Bearer token** authentication.

1. Open Chrome and go to https://bidartrader.ir/
2. Log in with your credentials
3. Press `F12` → **Network** tab
4. Look for requests to `api.bidartrader.ir`
5. Find `Authorization:` in Request Headers
6. Copy everything after `Bearer ` (just the token)
7. Paste in `config_bidar.json` → `authorization` field

**Note:** You may also need to copy the `x-user-trace` header value.

---

## Usage

```bash
# Build
cargo build --release

# Run for MofidOnlinePlus
cargo run --release -- mofid_online_plus

# Run for Danayan
cargo run --release -- danayan

# Run for Online Plus brokers
cargo run --release -- online_plus

# Run for Easy Trader
cargo run --release -- easy_trader

# Run for Exir brokers
cargo run --release -- exir

# Run for Bidar Trader
cargo run --release -- bidar

# Run ALL brokers in parallel
cargo run --release -- all
```

### Test Mode

Add `test` argument to run the loop only once (useful for testing configuration):

```bash
# Test single broker (runs once and exits)
cargo run --release -- mofid_online_plus test

# Test all brokers (each runs once and exits)
cargo run --release -- all test
```

Test mode output:
```
*** TEST MODE: Loop will run only once ***

Starting Sarkhati - MofidOnlinePlus Order Sender
...
=== Batch #1: Sending 1 orders ===
✓ Batch #1, Order #1: Sent successfully
[MofidOnlinePlus] Test mode: exiting after one batch
```

### Expected Output

```
Starting Sarkhati - MofidOnlinePlus Order Sender
Using Authorization header
Authorization preview: Bearer eyJhbGciOiJSUzI1NiIsImtpZCI6...
Loaded 1 order(s) from config
Batch delay: 100ms between batches
Starting continuous order sending...

=== Batch #1: Sending 1 orders ===
Sending order JSON: {"orderSide":"Buy","price":2474,...}
Order response status: 200 OK
✓ Batch #1, Order #1: Sent successfully
```

---

## Important Notes

⚠️ **This application will continuously send orders in a loop!**

- Understand what orders you're sending
- Monitor your account activity
- Use appropriate price and quantity values
- Press `Ctrl+C` to stop

⚠️ **Security:**

- Never commit config files with real credentials
- Never share your cookies or tokens
- Credentials are stored in plain text - secure your machine

⚠️ **Token Expiration:**

- Authorization tokens typically expire after 1-6 hours
- Cookies may last hours to days
- Refresh credentials when you get 401 errors

---

## Troubleshooting

### 401 Unauthorized
- Your credentials have expired
- Extract fresh cookie or token and update config

### 403 Forbidden
- Missing or incomplete cookie
- Make sure you copied the full cookie string

### Connection errors
- Check internet connection
- Verify broker services are online

---

## Disclaimer

This software is provided as-is for educational purposes. Use at your own risk. The authors are not responsible for any financial losses or account issues.

## License

MIT
