use std::process;

#[tokio::main]
async fn main() {
    match hifi_rs::cli::run().await {
        Ok(()) => println!("goodbye."),
        Err(err) => {
            println!("{err}");
            process::exit(1);
        }
    }
}
