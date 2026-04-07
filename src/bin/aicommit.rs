#[tokio::main]
async fn main() -> anyhow::Result<()> {
    aicommit::run().await
}
