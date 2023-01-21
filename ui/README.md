# Mu - UI

## Library Docs
- Json RPC method to get accounts information: https://docs.solana.com/developing/clients/jsonrpc-api#getaccountinfo
- JS Solana-web3 library: https://docs.solana.com/developing/clients/javascript-api

## Running local solana test validator to play with API(s):
1. Install Rust tools:
   https://rustup.rs/

2. Install Solana:
   - `sh -c "$(curl -sSfL https://release.solana.com/stable/install)"`
   - `solana-keygen new`

3. Install Anchor:
   - `cargo install --git https://github.com/coral-xyz/anchor avm --locked --force`
   - `avm install latest; avm use latest`

4. Clone Mu repo

5. Go to marketplace folder

6. Install Dependencies:
   - `npm install`

7. Build smart contract:
   - `anchor build`

8. Run local test-validator and deploy smart contract:
   - `anchor run deploy-test-stack -- -y`

9. For using `@project-serum/anchor` package you must set the `ANCHOR_WALLET` variable to `/home/$USER/.config/solana/id.json`

## CLI commands:
Go to cli folder

`cargo run list provider`