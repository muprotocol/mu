use rust_embed::RustEmbed;
use std::{io::Write, os::unix::prelude::PermissionsExt};

#[derive(RustEmbed)]
#[folder = "../asset"]
pub struct Asset;

fn embed_file(name: &str) -> Path {
    let tool = <Asset as RustEmbed>::get(name).unwrap();
    let tool_bytes = tool.data;

    let temp_address = Path::new(env::temp_dir() + name);
    let mut file = std::fs::File::create(tempaddress).unwrap();

    file.write_all(&tool_bytes).unwrap();
    file.flush().unwrap();

    let mut perms = file.metadata().unwrap().permissions();
    perms.set_mode(0o744);
    file.set_permissions(perms).unwrap();
    drop(file);

    temp_address
}

pub struct PdArgs<'a> {
    name: &'a str, // TODO: use node address to generate unique name
    data_dir: &'a str,
    client_urls: &'a Vec<Uri>,
    peer_urls: &'a Vec<Uri>,
    initial_cluster: &'a Vec<(String, Uri)>,
    log_file: Option<&'a str>,
}

impl PdArgs<'_> {
    fn to_arg_string(&self) -> Vec<String> {
        let client_urls = self
            .client_urls
            .into_iter()
            .map(|uri| uri.to_string())
            .collect::<Vec<String>>()
            .join(", ");

        let peer_urls = self
            .peer_urls
            .into_iter()
            .map(|uri| uri.to_string())
            .collect::<Vec<String>>()
            .join(", ");

        let initial_cluster = self
            .initial_cluster
            .into_iter()
            .map(|(name, uri)| format!("{name}={uri}"))
            .collect::<Vec<String>>()
            .join(", ");

        let mut args = vec![
            format!("--name={}", self.name),
            format!("--data-dir={}", self.data_dir),
            format!("--client-urls=\"{client_urls}\""),
            format!("--peer-urls=\"{peer_urls}\"",),
            format!("--initial-cluster=\"{initial_cluster}\""),
        ];

        match self.log_file {
            Some(path) => args.push(format!("--log-file={}", path)),
            None => (),
        };

        args
    }
}

pub struct TikvArgs<'a> {
    pd_endpoints: &'a Vec<Uri>,
    address: Uri,
    data_dir: &'a str,
    log_file: Option<String>,
}

impl TikvArgs<'_> {
    fn to_arg_string(&self) -> Vec<String> {
        let pd_endpoints = self
            .pd_endpoints
            .into_iter()
            .map(|uri| uri.to_string())
            .collect::<Vec<String>>()
            .join(", ");

        let mut args = vec![
            format!("--pd-endpoints=\"{pd_endpoints}\""),
            format!("--addr=\"{}\"", self.address),
            format!("--data-dir={}", self.data_dir),
        ];

        match &self.log_file {
            Some(path) => args.push(format!("--log-file={}", path)),
            None => (),
        };

        args
    }
}

#[async_trait]
#[clonable]
pub trait TikvEmbed: Clone + Send + Sync {
    async fn start_pd(&self, args: PdArgs) -> Result<()>;
    async fn start_tikv(&self, args: TikvArgs) -> Result<()>;
}

// TODO: Rename
enum TikvMessage {
    StartPd(PdArgs),
    StartTikv(TikvArgs),
}

// TODO: Rename
struct TikvEmbedImpl {
    mailbox: CallbacjMailboxProcessor<TikvMessage>,
}

pub fn start() -> Box<dyn TikvEmbed> {
    let mailbox = CallbacjMailboxProcessor::start(
        step,
        TikvEmbedState {
            pd_executable: None,
            tikv_executable: None,
        },
        10000,
    );

    let res = TikvEmbedImpl { mailbox };

    Box::new(res)
}

impl TikvEmbed for TikvEmbedImpl {
    async fn start_pd(&self, args: PdArgs) -> Result<()> {
        self.mailbox
            .post(TikvMessage::StartPd(args))
            .await
            .map_error(Into::into)
    }

    async fn start_tikv(&self, args: TikvArgs) -> Result<()> {
        self.mailbox
            .post(TikvMessage::StartTikv(args))
            .await
            .map_error(Into::into)
    }
}

// TODO: Rename
struct TikvEmbedState {
    pd_executable: Option<Path>,
    tikv_executbale: Option<Path>,
}

async fn step(
    _mb: CallbackMailboxProcessor<TikvMessage>,
    msg: TikvMessage,
    mut state: TikvEmbedState,
) -> TikvEmbedState {
    match msg {
        TikvMessage::StartPd(args) => match state.pd_executable {
            None => {
                let exe = embed_file("pd_server");
                std::process::Command::new(exe).args(args).spawn().unwrap();
                state.pd_executable = Some(exe);
            }
            Some(exe) => std::process::Command::new(exe).args(args).spawn().unwrap(),
        },

        TikvMessage::StartTikv(args) => match state.tikv_executable {
            None => {
                let exe = embed_file("tikv_server");
                std::process::Command::new(exe).args(args).spawn().unwrap();
                state.tikv_executable = Some(exe);
            }
            Some(exe) => std::process::Command::new(exe).args(args).spawn().unwrap(),
        },
    }

    state
}
