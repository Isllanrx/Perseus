mod comments;
mod lexers;

const USAGE: &str = "uso: cargo xtask <comando>

comandos:
  comments [--strip] [--report <arquivo.md>]
      Falha se algum arquivo de codigo versionado tiver comentario que nao seja diretiva de ferramenta
      de uma linha. --strip remove os comentarios (lexer por linguagem, sem regex), grava cada arquivo so
      depois de provar que as linhas de codigo nao mudaram e lista o texto removido no relatorio
      (padrao: target/comments-removed.md).
  help
      Mostra esta ajuda.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("comments") => comments::run(&args[1..]),
        Some("help" | "--help" | "-h") => println!("{USAGE}"),
        other => {
            if let Some(command) = other {
                eprintln!("comando desconhecido: {command}\n");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}
