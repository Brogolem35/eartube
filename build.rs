// Source - https://stackoverflow.com/a/65393488
// Posted by Fenhl, modified by community. See post 'Timeline' for change history
// Retrieved 2026-07-30, License - CC BY-SA 4.0

use {
	std::{env, io},
	winresource::WindowsResource,
};

fn main() -> io::Result<()> {
	if env::var_os("CARGO_CFG_WINDOWS").is_some() {
		WindowsResource::new()
			// This path can be absolute, or relative to your crate root.
			.set_icon("assets/icon.ico")
			.compile()?;
	}
	Ok(())
}
