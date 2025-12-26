mod architecture;
mod encoding;
mod formatting;
mod index;
mod server;
mod text_utils;
mod types;

use index::load_isa_index;
use server::IsaServer;
use tower_lsp::{LspService, Server};

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let (index, special_registers, load_info) = load_isa_index();
  let stdin = tokio::io::stdin();
  let stdout = tokio::io::stdout();
  let (service, socket) =
    LspService::new(|client| IsaServer::new(client, index, special_registers, load_info));
  Server::new(stdin, stdout, socket).serve(service).await;
}
