mod console;
mod effect;
mod fixture;
mod utils;

use console::Console;

#[tokio::main]
async fn main() {
    let mut halo_console = Console::new();
    halo_console.run().await;
}
