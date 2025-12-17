# Sarkhati - Automated Order Sender

A Rust application that automatically sends trading orders to Iranian stock brokers.

## Supported Brokers

- **Mofid Online** (mofid) - https://tg.mofidonline.com
- **BMI Bourse** (bmi) - https://online.bmibourse.ir
- **Danayan** (danayan) - https://trader.danayan.broker
- **Ordibehesht** (ordibehesht) - https://online.oibourse.ir
- **Alvand** (alvand) - https://arzeshafarin.exirbroker.com
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
# For Mofid Online
cp config_mofid.example.json config_mofid.json

# For BMI Bourse
cp config_bmi.example.json config_bmi.json

# For Danayan
cp config_danayan.example.json config_danayan.json

# For Ordibehesht
cp config_ordibehesht.example.json config_ordibehesht.json

# For Alvand
cp config_alvand.example.json config_alvand.json

# For Bidar Trader
cp config_bidar.example.json config_bidar.json
```

### 2. Get your authentication credentials

See [Authentication Guide](#authentication-guide) below.

### 3. Build and run:

```bash
cargo build --release

# For Mofid Online
cargo run --release -- mofid

# For BMI Bourse
cargo run --release -- bmi

# For Danayan
cargo run --release -- danayan

# For Ordibehesht
cargo run --release -- ordibehesht

# For Alvand
cargo run --release -- alvand

# For Bidar Trader
cargo run --release -- bidar

# For ALL brokers in parallel
cargo run --release -- all
```

Press `Ctrl+C` to stop.

---

## Configuration

### Mofid Online (`config_mofid.json`)

```json
{
  "cookie": "",
  "authorization": "YOUR_BEARER_TOKEN",
  "user_agent": "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0",
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
```

#### Mofid Order Parameters

| Field | Description |
|-------|-------------|
| `orderSide` | `"Buy"` or `"Sell"` |
| `price` | Order price |
| `quantity` | Number of shares |
| `symbolIsin` | Stock ISIN code (e.g., `"IRO1NMAD0001"`) |
| `validityType` | `0` for day order |
| `validityDate` | `null` for day orders |
| `orderFrom` | Platform identifier (`"Titan"`) |

### BMI Bourse (`config_bmi.json`)

```json
{
  "cookie": "YOUR_COOKIE_HERE",
  "user_agent": "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0",
  "order_url": "https://api2.bmibourse.ir/Web/V1/Order/Post",
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
```

#### BMI Order Parameters

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

### Danayan (`config_danayan.json`)

```json
{
  "cookie": "YOUR_COOKIE_HERE",
  "user_agent": "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0",
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

### Ordibehesht (`config_ordibehesht.json`)

```json
{
  "cookie": "YOUR_COOKIE_HERE",
  "user_agent": "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0",
  "order_url": "https://api.oibourse.ir/Web/V1/Order/Post",
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
```

#### Ordibehesht Order Parameters

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

### Alvand (`config_alvand.json`)

```json
{
  "cookie": "YOUR_COOKIE_HERE",
  "user_agent": "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0",
  "order_url": "https://arzeshafarin.exirbroker.com/api/v1/order",
  "x_app_n": "1824632377792.35566496",
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
```

#### Alvand Order Parameters

| Field | Description |
|-------|-------------|
| `side` | `"SIDE_BUY"` or `"SIDE_SELL"` |
| `price` | Order price |
| `quantity` | Number of shares |
| `insMaxLcode` | Stock ISIN code |
| `orderType` | `"ORDER_TYPE_LIMIT"` for limit order |
| `validityType` | `"VALIDITY_TYPE_DAY"` for day order |
| `validityDate` | Empty string for day orders |
| `coreType` | Usually `"c"` |
| `bankAccountId` | Usually `-1` |

### Bidar Trader (`config_bidar.json`)

```json
{
  "authorization": "YOUR_BEARER_TOKEN_HERE",
  "user_agent": "Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0",
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

### Mofid Online

Mofid supports both **Cookie** and **Bearer token** authentication.

#### Option A: Bearer Token (Recommended)

1. Open Chrome and go to https://tg.mofidonline.com/
2. Log in with your credentials
3. Press `F12` to open Developer Tools
4. Click the **Network** tab
5. Look for requests to `mofidonline.com/apigateway`
6. Find the `Authorization` header in Request Headers
7. Copy everything after `Bearer ` (just the token)
8. Paste in `config_mofid.json` → `authorization` field

#### Option B: Cookie

1. Open Chrome and go to https://tg.mofidonline.com/
2. Log in with your credentials
3. Press `F12` → **Network** tab → Refresh page
4. Click on a request to `tg.mofidonline.com`
5. Find `Cookie:` in Request Headers
6. Copy the entire cookie string
7. Paste in `config_mofid.json` → `cookie` field

### BMI Bourse

BMI uses **Cookie** authentication only.

1. Open Chrome and go to https://online.bmibourse.ir/
2. Log in with your credentials
3. Press `F12` → **Network** tab
4. Look for requests to `api2.bmibourse.ir`
5. Find `Cookie:` in Request Headers
6. Copy the entire cookie string
7. Paste in `config_bmi.json` → `cookie` field

### Danayan

Danayan uses **Cookie** authentication (contains embedded Authorization token).

1. Open Chrome and go to https://trader.danayan.broker/
2. Log in with your credentials
3. Press `F12` → **Network** tab
4. Look for requests to `otapi.danayan.broker`
5. Find `Cookie:` in Request Headers
6. Copy the entire cookie string (includes `Authorization=Bearer%20...`)
7. Paste in `config_danayan.json` → `cookie` field

### Ordibehesht

Ordibehesht uses **Cookie** authentication only.

1. Open Chrome and go to https://online.oibourse.ir/
2. Log in with your credentials
3. Press `F12` → **Network** tab
4. Look for requests to `api.oibourse.ir`
5. Find `Cookie:` in Request Headers
6. Copy the entire cookie string
7. Paste in `config_ordibehesht.json` → `cookie` field

### Alvand

Alvand uses **Cookie** authentication (contains JWT-TOKEN).

1. Open Chrome and go to https://arzeshafarin.exirbroker.com/
2. Log in with your credentials
3. Press `F12` → **Network** tab
4. Look for requests to `arzeshafarin.exirbroker.com/api`
5. Find `Cookie:` in Request Headers
6. Copy the entire cookie string (includes `JWT-TOKEN=...`)
7. Paste in `config_alvand.json` → `cookie` field

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

# Run for Mofid Online
cargo run --release -- mofid

# Run for BMI Bourse
cargo run --release -- bmi

# Run for Danayan
cargo run --release -- danayan

# Run for Ordibehesht
cargo run --release -- ordibehesht

# Run for Alvand
cargo run --release -- alvand

# Run for Bidar Trader
cargo run --release -- bidar

# Run ALL brokers in parallel
cargo run --release -- all
```

### Test Mode

Add `test` argument to run the loop only once (useful for testing configuration):

```bash
# Test single broker (runs once and exits)
cargo run --release -- mofid test

# Test all brokers (each runs once and exits)
cargo run --release -- all test
```

Test mode output:
```
*** TEST MODE: Loop will run only once ***

Starting Sarkhati - Mofid Online Order Sender
...
=== Batch #1: Sending 1 orders ===
✓ Batch #1, Order #1: Sent successfully
[Mofid] Test mode: exiting after one batch
```

### Expected Output

```
Starting Sarkhati - Mofid Online Order Sender
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

