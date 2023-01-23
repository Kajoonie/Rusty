use rusty::Rusty;

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to load .env file");
    let rusty = Rusty {};
    if let Err(e) = rusty.start().await {
        panic!("Unable to start: {e:?}")
    }
}
