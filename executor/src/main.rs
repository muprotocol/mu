fn main() -> anyhow::Result<()> {
    let body = async { mu::run().await };

    #[allow(clippy::expect_used, clippy::diverging_sub_expression)]
    {
        return tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .max_blocking_threads(2048) //TODO: Make this configurable
            .build()
            .expect("Failed building the Runtime")
            .block_on(body);
    }
}
