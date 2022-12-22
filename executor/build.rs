// TODO: grab tikv and pd from *somewhere* and place them into assets

const TIKV_VERSION: &str = "6.4.0";

fn main() {
    let pd_url = format!("https://tiup-mirrors.pingcap.com/pd-v{TIKV_VERSION}-linux-amd64.tar.gz");
    let tikv_url =
        format!("https://tiup-mirrors.pingcap.com/tikv-v{TIKV_VERSION}-linux-amd64.tar.gz");

    let client = reqwest::blocking::Client::default();
    let pd_bytes = client.get(pd_url).send().unwrap().bytes();
}
