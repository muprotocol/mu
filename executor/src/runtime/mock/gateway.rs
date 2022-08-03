use super::super::message::gateway::{GatewayRequest, GatewayResponse};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time;

pub async fn start(
    interval: Duration,
    count: usize,
    f: fn(usize) -> GatewayRequest,
) -> mpsc::Receiver<(GatewayRequest, oneshot::Sender<GatewayResponse>)> {
    let (req_tx, req_rx) = mpsc::channel(count);

    let mut interval = time::interval(interval);
    let mut join_handles = vec![];

    for i in 0..count {
        interval.tick().await;
        let req_tx = req_tx.clone();
        join_handles.push(tokio::spawn(async move {
            let (resp_tx, resp_rx) = oneshot::channel::<GatewayResponse>();

            let request = f(i);
            println!("mock-gateway: [TX] ({}): {:?}", i, request);
            req_tx.send((request, resp_tx)).await.unwrap();

            let response = resp_rx.await.unwrap();
            println!("mock-gateway: [RX] ({}): {:?}", i, response);
        }));
    }

    tokio::spawn(async move {
        for join_handle in join_handles.drain(..) {
            join_handle.await.unwrap();
        }
    });

    req_rx
}
