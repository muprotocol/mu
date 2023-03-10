use musdk::*;

#[mu_functions]
mod functions {
    use musdk::{LogLevel, MuContext, PathParams};

    #[mu_function]
    fn greet_user<'a>(ctx: &'a mut MuContext, data: &'a [u8]) -> Vec<u8> {
        let s = String::from_utf8_lossy(data);

        let mut count = ctx
            .db()
            .get("t1", data)
            .unwrap()
            .map(|v| v.0)
            .unwrap_or_default()
            .into_iter()
            .next()
            .unwrap_or_default();
        count = count.wrapping_add(1);
        ctx.db().put("t1", data, vec![count], false).unwrap();
        ctx.db().put("t2", "x", [0u8], false).unwrap();

        ctx.log("storage is up and running", LogLevel::Info)
            .unwrap();

        ctx.storage()
            .put("test_storage", "test_file.txt", data)
            .unwrap();

        ctx.log("successfully uploaded to storage", LogLevel::Info)
            .unwrap();

        let mut storage = ctx.storage();

        let received_data = storage.get("test_storage", "test_file.txt").unwrap();
        assert!(received_data == data);

        ctx.log(
            "successfully downloaded from storage and validated the results",
            LogLevel::Info,
        )
        .unwrap();

        ctx.storage()
            .delete("test_storage", "test_file.txt")
            .unwrap();

        let _ = ctx.log(&format!("Received request from {s}"), LogLevel::Info);
        format!("(#{count}) Hello, {s}!").into_bytes()
    }

    #[mu_function]
    fn greet_path_user<'a>(ctx: &'a mut MuContext, path: PathParams<'a>) -> Vec<u8> {
        let name = path.get("name").expect("Expected to have name path param");

        let _ = ctx.log(&format!("Received request from {name}"), LogLevel::Info);
        format!("Hello, {name}!").into_bytes()
    }
}
