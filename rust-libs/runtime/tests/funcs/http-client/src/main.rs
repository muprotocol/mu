use musdk::*;

#[mu_functions]
mod http_client {
    use super::*;

    #[mu_function]
    fn test_download<'a>(ctx: &'a mut MuContext) -> Vec<u8> {
        match ctx.http_client().get("http://example.com").send() {
            Err(client_error) => ctx
                .log(format!("client error: {client_error:?}"), LogLevel::Debug)
                .unwrap(),
            Ok(response) => match response {
                Err(http_error) => ctx
                    .log(format!("http error: {http_error:?}"), LogLevel::Debug)
                    .unwrap(),
                Ok(response) => {
                    assert_eq!(response.status, Status::Ok);
                    return response.body.to_vec();
                }
            },
        }

        b"Failed to sent http request".to_vec()
    }
}
