use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(
    version,
    about = "CLI tool for working with Mu stacks, for internal use only"
)]
enum Command {
    YamlToProto {
        #[arg(
            short,
            long,
            help = "Input file name, will read from stdin if not provided"
        )]
        in_file: Option<String>,

        #[arg(
            short,
            long,
            help = "Output file name, will write to stdout if not provided"
        )]
        out_file: Option<String>,
    },

    ProtoToYaml {
        #[arg(
            short,
            long,
            help = "Input file name, will read from stdin if not provided"
        )]
        in_file: Option<String>,

        #[arg(
            short,
            long,
            help = "Output file name, will write to stdout if not provided"
        )]
        out_file: Option<String>,
    },
}

fn read_file_or_stdin(path: &Option<String>) -> Result<String> {
    match path.as_deref() {
        None | Some("") => {
            let stdin = std::io::stdin();

            Ok(stdin
                .lines()
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|x| format!("{x}\n"))
                .collect::<String>())
        }
        Some(path) => Ok(std::fs::read_to_string(path)?),
    }
}

fn write_file_or_stdout(path: &Option<String>, contents: impl AsRef<str>) -> Result<()> {
    match path.as_deref() {
        None | Some("") => {
            println!("{}", contents.as_ref());
            Ok(())
        }
        Some(path) => Ok(std::fs::write(path, contents.as_ref())?),
    }
}

fn main() -> anyhow::Result<()> {
    let command = Command::parse();

    match command {
        Command::YamlToProto { in_file, out_file } => {
            let yaml = read_file_or_stdin(&in_file)?;
            let stack: mu_stack::Stack = serde_yaml::from_str(yaml.as_ref())?;
            let proto = stack.serialize_to_proto()?;
            let base64 = base64::encode(proto);
            write_file_or_stdout(&out_file, base64)?;
        }

        Command::ProtoToYaml { in_file, out_file } => {
            let base64 = read_file_or_stdin(&in_file)?;
            let proto = base64::decode(base64.trim())?;
            let stack = mu_stack::Stack::try_deserialize_proto(bytes::Bytes::from(proto))?;
            let yaml = serde_yaml::to_string(&stack)?;
            write_file_or_stdout(&out_file, yaml)?;
        }
    }

    Ok(())
}
