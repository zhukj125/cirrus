#[tokio::main(flavor = "multi_thread")]
async fn main() {
    cirrus_mcp::run_cli().await;
}
