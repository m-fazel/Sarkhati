# Sarkhati - Automated Order Sender

A Rust application that automatically sends trading orders to the Mofid Online platform.

## Features

- Cookie-based authentication (simple and reliable)
- Continuous order sending in a loop
- Configuration-based order parameters
- Automatic retry logic on errors

## Prerequisites

- Rust (latest stable version)
- Valid Mofid Online account
- Browser cookies from an active session

## Configuration

Edit the `config.json` file with your authentication credentials and order parameters:

```json
{
  "cookie": "YOUR_COOKIES_HERE",
  "authorization": "",
  "order": {
    "orderSide": "Buy",
    "price": 8656,
    "quantity": 1,
    "symbolIsin": "IRO3HANI0001",
    "validityType": 0,
    "validityDate": null,
    "orderFrom": "Titan"
  }
}
```

### Authentication Methods

The application supports two authentication methods:

1. **Cookie-based (Recommended)**: Set the `cookie` field with your browser cookies
2. **Authorization header**: Set the `authorization` field with your bearer token (without the "Bearer " prefix)

**Priority**: If `cookie` is set and valid, it will be used. Otherwise, `authorization` will be used.

**Important**: You must set at least one authentication method. See the sections below for how to obtain these values.

### Order Parameters

- `orderSide`: "Buy" or "Sell"
- `price`: Order price
- `quantity`: Number of shares
- `symbolIsin`: Stock symbol ISIN code
- `validityType`: Order validity type (0 for day order)
- `validityDate`: Expiration date (null for day orders)
- `orderFrom`: Platform identifier (use "Titan")

---

## Quick Start (3 Minutes)

### Step 1: Get Your Authentication (1 minute)

**Option A: Using Cookies (Recommended)**

1. Open Chrome and go to https://tg.mofidonline.com/
2. Log in with your credentials
3. Press `F12` to open Developer Tools
4. Click the **Network** tab
5. Press `F5` to refresh the page
6. Click on the first request to `tg.mofidonline.com`
7. In the **Headers** section, find `Cookie:` in Request Headers
8. Copy everything after `Cookie: ` (the entire line)

**Option B: Using Authorization Header**

1. Open Chrome and go to https://tg.mofidonline.com/
2. Log in with your credentials
3. Press `F12` to open Developer Tools
4. Click the **Network** tab
5. Look for a request to `mofidonline.com/apigateway`
6. In the **Headers** section, find `Authorization:` in Request Headers
7. Copy everything after `Bearer ` (just the token, not the word "Bearer")

### Step 2: Update config.json (30 seconds)

Open `config.json` and:
- If using cookies: Replace `PASTE_YOUR_COOKIE_HERE` with the cookie you copied
- If using authorization: Paste the token in the `authorization` field and clear the `cookie` field

### Step 3: Build and Run (1 minute)

```bash
cargo build --release
cargo run --release
```

You should see (with cookies):

```
Starting Sarkhati - Order Sender
Using Cookie authentication
Cookie preview: _ga=GA1.1.650223579.1640070824; _hjSessionUser...
Sending order: OrderData { order_side: "Buy", price: 8656, quantity: 1, ... }
Order response status: 200
Order response body: {"success": true, ...}
✓ Order sent successfully
Waiting 1 second before next order...
```

Or (with authorization):

```
Starting Sarkhati - Order Sender
Using Authorization header
Authorization preview: Bearer eyJhbGciOiJSUzI1NiIsImtpZCI6...
Sending order: OrderData { order_side: "Buy", price: 8656, quantity: 1, ... }
Order response status: 200
Order response body: {"success": true, ...}
✓ Order sent successfully
Waiting 1 second before next order...
```

Press `Ctrl+C` to stop.

---

## Getting Your Authentication Credentials (Detailed Guide)

The application supports two authentication methods: cookies or authorization header.

### Method 1: Cookie Authentication (Recommended)

#### Using Chrome Developer Tools

1. **Open Chrome** and go to https://tg.mofidonline.com/

2. **Log in** with your credentials

3. **Open Developer Tools**:
   - Press `F12` on your keyboard, OR
   - Right-click anywhere on the page → "Inspect", OR
   - Menu → More Tools → Developer Tools

4. **Go to the Network tab**:
   - Click on the "Network" tab in Developer Tools
   - Make sure "Preserve log" is checked

5. **Refresh the page**:
   - Press `F5` or click the refresh button
   - You'll see network requests appearing

6. **Find a request to the main domain**:
   - Look for a request to `tg.mofidonline.com` (usually the first one)
   - Click on it

7. **Copy the Cookie header**:
   - In the "Headers" section, scroll down to "Request Headers"
   - Find the line that starts with `Cookie:`
   - Copy **everything after** `Cookie: ` (all the way to the end)

8. **Paste into config.json**:
   - Open `config.json`
   - Replace `PASTE_YOUR_COOKIE_HERE` with the cookie you copied
   - Make sure it's in quotes

#### Using Application Tab (Alternative)

1. **Open Chrome** and go to https://tg.mofidonline.com/

2. **Log in** with your credentials

3. **Open Developer Tools** (F12)

4. **Go to Application tab**:
   - Click on "Application" tab
   - In the left sidebar, expand "Cookies"
   - Click on "https://tg.mofidonline.com"

5. **Copy all cookies**:
   - You'll see a list of cookies
   - You need to format them as: `name1=value1; name2=value2; name3=value3`
   - Important cookies to include:
     - `IO01072114`
     - `IO011e8d5c`
     - `IO01cb891d`
     - `_ga`
     - `_hjSessionUser_3316863`

6. **Format example**:
   ```
   _ga=GA1.1.650223579.1640070824; IO01072114=01b53d46e100b3f5be900bedd8ac711867fe71f77d4fbf0c6b1525e72f1adea48dd94a1aff6d17d876cc626abc530165ddbc9c86cb; IO011e8d5c=01b53d46e1c5027d73f3eca591a682261240b1e5383a73aadf7f0133955257220807a18b8efe45f6c6697e7775c0d8213f9b119355; IO01cb891d=01b53d46e1629bdc9844c840c8dfeb1c691b040144d045a90a44e7e1a704d1071821efc72064693b0fcb2c7650bcee9942e8699d58
   ```

### Method 2: Authorization Header Authentication

1. **Open Chrome** and go to https://tg.mofidonline.com/

2. **Log in** with your credentials

3. **Open Developer Tools** (F12)

4. **Go to the Network tab**:
   - Click on the "Network" tab in Developer Tools
   - Make sure "Preserve log" is checked

5. **Perform an action** that triggers an API call:
   - Click on any trading action or navigate to a different page
   - Look for requests to `mofidonline.com/apigateway`

6. **Find the Authorization header**:
   - Click on a request to `mofidonline.com/apigateway`
   - In the "Headers" section, scroll down to "Request Headers"
   - Find the line that starts with `Authorization: Bearer `
   - Copy **everything after** `Bearer ` (just the token, not the word "Bearer")

7. **Paste into config.json**:
   - Open `config.json`
   - Paste the token in the `authorization` field
   - Clear or remove the `cookie` field (set it to empty string `""`)

### Example config.json Files

**Using Cookie Authentication:**
```json
{
  "cookie": "_ga=GA1.1.650223579.1640070824; _hjSessionUser_3316863=....",
  "authorization": "",
  "order": {
    "orderSide": "Buy",
    "price": 8656,
    "quantity": 1,
    "symbolIsin": "IRO3HANI0001",
    "validityType": 0,
    "validityDate": null,
    "orderFrom": "Titan"
  }
}
```

**Using Authorization Header:**
```json
{
  "cookie": "",
  "authorization": "eyJhbGciOiJSUzI1N...",
  "order": {
    "orderSide": "Buy",
    "price": 8656,
    "quantity": 1,
    "symbolIsin": "IRO3HANI0001",
    "validityType": 0,
    "validityDate": null,
    "orderFrom": "Titan"
  }
}
```

**Note**: Do NOT include the word "Bearer" in the authorization field - just the token itself.

---

## Usage Guide

### Setup

1. **Copy the example configuration:**
   ```bash
   cp config.example.json config.json
   ```

2. **Get your cookies** (see "Getting Your Cookies" section above)

3. **Edit `config.json`** with your cookies and order parameters

4. **Build the project:**
   ```bash
   cargo build --release
   ```

5. **Run the application:**
   ```bash
   cargo run --release
   ```

### What Happens

The application will:

1. Read your cookies and order parameters from `config.json`
2. Start sending orders in a continuous loop (1 second between orders)
3. Automatically retry if errors occur (5 second delay)
4. Print status messages for each order

### Expected Output

```
Starting Sarkhati - Order Sender
Cookie preview: _ga=GA1.1.650223579.1640070824; _hjSessionUser...
Sending order: OrderData { order_side: "Buy", price: 8656, quantity: 1, ... }
Order response status: 200
Order response body: {"success": true, ...}
✓ Order sent successfully
Waiting 1 second before next order...
Sending order: OrderData { order_side: "Buy", price: 8656, quantity: 1, ... }
Order response status: 200
Order response body: {"success": true, ...}
✓ Order sent successfully
Waiting 1 second before next order...
...
```

### Stopping the Application

Press `Ctrl+C` to stop the application.

### Modifying Order Parameters

You can change order parameters in `config.json` without rebuilding:

- `orderSide`: "Buy" or "Sell"
- `price`: The order price
- `quantity`: Number of shares
- `symbolIsin`: Stock ISIN code (e.g., "IRO3HANI0001")

Restart the application after making changes.

---

## How It Works

1. **Authentication**:
   - Uses browser cookies from your active session
   - No complex OAuth flow needed
   - Simple and reliable

2. **Order Loop**: The application:
   - Sends POST requests to the order endpoint
   - Uses your cookies for authentication
   - Continuously sends orders in a loop (1 second between orders)
   - Automatically retries on errors (5 second delay)

3. **Cookie Refresh**: Cookies expire after some time (hours to days)
   - When you get 401 errors, extract fresh cookies
   - Update `config.json` with the new cookies
   - Restart the application

## Important Notes

⚠️ **This application will continuously send orders in a loop!**

- Make sure you understand what orders you're sending
- Monitor your account activity
- Use appropriate price and quantity values
- Consider the financial implications
- Press `Ctrl+C` to stop the application

⚠️ **Security Considerations:**

- Never commit `config.json` with real cookies to version control
- Never share your cookies with anyone - they give full access to your account
- Store credentials securely
- Use environment variables for sensitive data in production
- This tool is for educational/personal use only

## Troubleshooting

### Authentication Issues

**Problem**: "No authentication method configured" error
- **Solution**: You must set either `cookie` or `authorization` in config.json. See the authentication sections above.

**Problem**: Getting 401 Unauthorized errors
- **Solution**: Your authentication has expired. Extract fresh cookies or a new authorization token and update config.json

**Problem**: Getting 403 Forbidden errors
- **Solution**:
  - If using cookies: Make sure you copied ALL the cookies, including the session cookies (IO01...)
  - If using authorization: Make sure you copied the full token and didn't include the word "Bearer"

**Problem**: Application says "Cookie preview: PASTE_YOUR_COOKIE_HERE"
- **Solution**: You haven't updated config.json yet. Follow the steps in the authentication sections above.

### Cookie-Specific Issues

If you see authentication errors when using cookies:

1. Make sure you copied the FULL cookie string from the browser
2. Check that you're logged in to the website in your browser
3. Extract fresh cookies and update `config.json`
4. See the "Method 1: Cookie Authentication" section above for detailed instructions

### Authorization Header Issues

If you see authentication errors when using authorization header:

1. Make sure you copied the FULL token (it's very long, usually 800+ characters)
2. Do NOT include the word "Bearer" - just the token itself
3. Make sure the `cookie` field is empty or set to `""`
4. Authorization tokens typically expire after 1-6 hours - extract a fresh one when needed

### Authentication Expired

If you see 401 Unauthorized errors:

1. Your authentication credentials have expired
2. Log in to the website again in your browser
3. Extract fresh credentials using the appropriate method:
   - For cookies: See "Method 1: Cookie Authentication"
   - For authorization: See "Method 2: Authorization Header Authentication"
4. Update `config.json` with the new credentials
5. Restart the application

⚠️ **Expiration Times**:
- **Cookies**: Typically expire after hours to days
- **Authorization tokens**: Typically expire after 1-6 hours

⚠️ **Security**: Never share your cookies or authorization tokens with anyone! They give full access to your account.

⚠️ **Keep Browser Session Active**: For best results with cookies, keep your browser logged in while running the application.

### Order Errors

If orders fail:
- Check that the stock symbol (ISIN) is correct
- Verify the price and quantity are valid
- Ensure your account has sufficient funds
- Check market hours

### Network Errors

If you see network errors:

- Check your internet connection
- Verify that the Mofid Online services are accessible
- Check if there are any firewall or proxy issues

## Disclaimer

This software is provided as-is for educational purposes. Use at your own risk. The authors are not responsible for any financial losses or account issues that may result from using this software.

## License

MIT

