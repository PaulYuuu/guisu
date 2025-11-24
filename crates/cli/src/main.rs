//! Guisu CLI application entry point
//!
//! This is the minimal main entry point that delegates to the library.

use clap::Parser;

fn main() {
    // Configure miette for beautiful error reporting
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(false)
                .unicode(true)
                .context_lines(2)
                .tab_width(4)
                .build(),
        )
    }))
    .ok();

    // Parse CLI arguments
    let cli = guisu::Cli::parse();

    // Run and display errors with miette formatting
    if let Err(e) = guisu::run(cli) {
        // Convert anyhow error to miette for beautiful display
        let miette_error = miette::Report::msg(format!("{e:#}"));
        eprintln!("{miette_error:?}");
        std::process::exit(1);
    }
}
