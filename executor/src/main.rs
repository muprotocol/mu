use mu::mu_stack::{Database, Function, Gateway, GatewayEndpoint, HttpMethod, Service, Stack};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // mu::run().await
    let v = Stack {
        name: "My stack".into(),
        version: "0.1.0".into(),
        services: vec![
            Service::Database(Database {
                name: "My DB".into(),
            }),
            Service::Function(Function {
                name: "My func".into(),
                binary: "http://oh.my.god/func.wasm".into(),
                env: [
                    ("yyz".into(), "abc".into()),
                    ("zyz".into(), "abc".into()),
                    ("xyz".into(), "abc".into()),
                ]
                .into(),
                runtime: mu::mu_stack::FunctionRuntime::Wasi1_0,
            }),
            Service::Gateway(Gateway {
                name: "My gateway".into(),
                endpoints: [(
                    "/login".into(),
                    vec![
                        GatewayEndpoint {
                            method: HttpMethod::Get,
                            route_to: "My func".into(),
                        },
                        GatewayEndpoint {
                            method: HttpMethod::Post,
                            route_to: "My func".into(),
                        },
                    ],
                )]
                .into(),
            }),
        ],
    };

    let s = serde_yaml::to_string(&v)?;

    println!("{s}");

    let s = r#"
- type: Database
  name: My DB
- type: Function
  name: My func
  binary: http://oh.my.god/func.wasm
  runtime: wasi1.0
  env:
    xyz: abc
- type: Gateway
  name: My gateway
  endpoints:
    /login:
    - method: get
      route_to: My func
    - method: post
      route_to: My func
    "#
    .to_string();

    let v2 = serde_yaml::from_str::<Vec<Service>>(s.as_str())?;

    println!("{v2:?}");

    Ok(())
}
