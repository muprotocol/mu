#[tokio::main]
async fn main() -> anyhow::Result<()> {
    mu::run().await

    //     let s = r#"
    // - type: Database
    //   name: My DB
    // - type: Function
    //   name: My func
    //   binary: http://oh.my.god/func.wasm
    //   runtime: wasi1.0
    //   env:
    //     xyz: abc
    // - type: Gateway
    //   name: My gateway
    //   endpoints:
    //     /login:
    //     - method: get
    //       route_to: My func
    //     - method: post
    //       route_to: My func
    //     "#
    //     .to_string();

    //     let v2 = serde_yaml::from_str::<Vec<Service>>(s.as_str())?;

    //     println!("{v2:?}");

    //     Ok(())
}
