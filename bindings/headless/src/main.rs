//! `kition-layout`: serves [`kition_layout::backend`] when Kition launches the kit.

fn main() -> std::process::ExitCode {
    match kition_layout::kit() {
        Ok(kit) => kition_headless::serve_backend(&kit, kition_layout::backend(&kit)),
        Err(error) => {
            eprintln!("kit: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
